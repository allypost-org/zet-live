use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, LazyLock},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use axum_client_ip::ClientIp;
use futures::{SinkExt, StreamExt};
use tokio::{sync::RwLock, time};
use tracing::{debug, error, trace, warn};

use super::{
    INITIAL_STATE, V1AppState,
    admin_notifications::{AdminNotification, NotificationTarget, get_admin_notification_receiver},
};
use crate::{
    auth::session,
    server::routes::v1::{Broadcast, Transmission, Versioned},
};

pub static WS_CONNECTIONS: LazyLock<Arc<RwLock<HashMap<IpAddr, u32>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "t", content = "d", rename_all = "kebab-case")]
enum ClientMessage {
    Auth(Option<String>),
}

#[derive(Debug, serde::Deserialize)]
struct ClientEnvelope {
    #[serde(rename = "v")]
    version: u64,
    #[serde(flatten)]
    message: ClientMessage,
}

const CLIENT_PROTOCOL_VERSION: u64 = 1;

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<V1AppState>>,
    ClientIp(ip): ClientIp,
) -> impl IntoResponse {
    ws.on_upgrade(move |stream| websocket(stream, ip, state))
}

async fn handle_admin_notification(
    notification: &AdminNotification,
    addr: IpAddr,
    user_id: Option<&str>,
    session_id: Option<&str>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> bool {
    match notification {
        AdminNotification::Toast {
            bytes,
            target,
            ips,
            account,
        } => {
            let should_send = match target {
                NotificationTarget::All => true,
                NotificationTarget::Ips => ips.contains(&addr),
                NotificationTarget::Account => account.as_deref() == user_id,
            };

            if !should_send {
                return true;
            }

            if sender
                .send(Message::Binary(Bytes::from(bytes.clone())))
                .await
                .is_err()
            {
                return false;
            }
        }
        AdminNotification::SessionRevoked {
            text,
            user_id: target_user,
            session_id: target_session,
        } => {
            if Some(target_user.as_str()) != user_id || Some(target_session.as_str()) != session_id
            {
                return true;
            }

            if sender
                .send(Message::Text(text.clone().into()))
                .await
                .is_err()
            {
                return false;
            }
        }
    }

    true
}

#[allow(clippy::too_many_lines)]
async fn websocket(stream: WebSocket, addr: IpAddr, state: Arc<V1AppState>) {
    trace!(?stream, "Websocket opened");
    debug!(?addr, "Websocket opened");
    WS_CONNECTIONS
        .write()
        .await
        .entry(addr)
        .and_modify(|x| *x += 1)
        .or_insert(1);
    let (mut sender, mut receiver) = stream.split();

    let mut user_id = None;
    let mut session_id = None;

    if let Err(e) = send_initial_state(&mut sender).await {
        error!(?e, "Error sending initial state");
        cleanup_connection(addr).await;
        return;
    }

    let mut ping_interval = {
        #[allow(clippy::cast_sign_loss)]
        let interval = (30_000 + rand::random_range(-5_000_i64..5_000)) as u64;
        time::interval(Duration::from_millis(interval))
    };
    let mut transmission_rx = state.get_transmission_receiver();
    let mut notification_rx = get_admin_notification_receiver();

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                debug!(?addr, "Pinging client");
                if sender
                    .send(Message::Ping(Bytes::from_static(&[1, 2, 3])))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            result = state.wait_for_transmission(&mut transmission_rx) => {
                let transmission = match result {
                    Ok(t) => t,
                    Err(e) => {
                        warn!(?e, "Error waiting for transmission");
                        break;
                    }
                };

                if !handle_transmission(&transmission, addr, user_id.as_deref(), &mut sender).await {
                    break;
                }
            }
            result = notification_rx.recv() => {
                let notification = match result {
                    Ok(n) => n,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        warn!(count, "Admin notification channel lagged");
                        continue;
                    }
                    Err(e) => {
                        warn!(?e, "Admin notification channel closed");
                        break;
                    }
                };

                if !handle_admin_notification(
                    &notification,
                    addr,
                    user_id.as_deref(),
                    session_id.as_deref(),
                    &mut sender,
                )
                .await
                {
                    break;
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(new_state) = handle_client_text(&text, addr, &mut sender).await {
                            match new_state {
                                AuthState::Authenticated { user_id: uid, session_id: sid } => {
                                    user_id = Some(uid);
                                    session_id = Some(sid);
                                }
                                AuthState::Unauthenticated => {
                                    user_id = None;
                                    session_id = None;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        debug!(?addr, "Client closed WS");
                        break;
                    }
                    other => {
                        trace!(?other, "Received unhandled message");
                    }
                }
            }
        }
    }

    cleanup_connection(addr).await;

    debug!(?addr, "Websocket closed");
}

async fn handle_transmission(
    transmission: &Transmission,
    addr: IpAddr,
    user_id: Option<&str>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> bool {
    match transmission {
        Transmission::BroadcastToAll(data) => {
            trace!(to = ?addr, "Broadcasting data");
            sender
                .send(Message::Binary(Bytes::clone(data)))
                .await
                .is_ok()
        }
        Transmission::UserNotice {
            user_id: target,
            bytes,
        } => {
            if Some(target.as_str()) != user_id {
                return true;
            }
            trace!(to = ?addr, "Sending per-account notice");
            sender
                .send(Message::Binary(Bytes::clone(bytes)))
                .await
                .is_ok()
        }
        Transmission::Empty => true,
    }
}

async fn cleanup_connection(addr: IpAddr) {
    let mut ws_connections = WS_CONNECTIONS.write().await;
    if let Some(count) = ws_connections.get_mut(&addr) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            ws_connections.remove(&addr);
        }
    }
}

enum AuthState {
    Authenticated { user_id: String, session_id: String },
    Unauthenticated,
}
async fn handle_client_text(
    text: &str,
    addr: IpAddr,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Option<AuthState> {
    let Ok(envelope) = serde_json::from_str::<ClientEnvelope>(text) else {
        warn!(?addr, "Malformed client message");
        return None;
    };
    if envelope.version != CLIENT_PROTOCOL_VERSION {
        warn!(
            version = envelope.version,
            ?addr,
            "Unsupported client protocol version"
        );
        return None;
    }
    match envelope.message {
        ClientMessage::Auth(Some(token)) => match session::lookup_session(&token).await {
            Ok(Some(session_row)) => {
                debug!(
                    ?addr,
                    user_id = %session_row.user_id,
                    session_id = %session_row.id,
                    "WS connection authenticated"
                );
                if let Err(e) = send_user_notices(sender, &session_row.user_id).await {
                    warn!(?e, ?addr, "Failed to send user notices after auth");
                }
                Some(AuthState::Authenticated {
                    user_id: session_row.user_id,
                    session_id: session_row.id,
                })
            }
            Ok(None) => {
                warn!(?addr, "Invalid auth token over WS");
                None
            }
            Err(e) => {
                error!(?e, ?addr, "DB error during WS auth");
                None
            }
        },
        ClientMessage::Auth(None) => {
            debug!(?addr, "WS connection deauthenticated");
            Some(AuthState::Unauthenticated)
        }
    }
}

async fn send_initial_state(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), axum::Error> {
    {
        let vehicles = INITIAL_STATE.vehicles().await.clone();

        let res = sender.send(Message::Binary(vehicles)).await;

        if let Err(e) = res {
            error!(?e, "Error sending initial vehicles");
            return Err(e);
        }
    }

    {
        let active_stops = INITIAL_STATE.active_stops().await.clone();

        let res = sender.send(Message::Binary(active_stops)).await;

        if let Err(e) = res {
            error!(?e, "Error sending initial active stops");
            return Err(e);
        }
    }

    {
        let notices = INITIAL_STATE.notices().await.clone();
        if !notices.is_empty() {
            let res = sender.send(Message::Binary(notices)).await;

            if let Err(e) = res {
                error!(?e, "Error sending initial notices");
                return Err(e);
            }
        }
    }

    {
        let gbfs_stations = INITIAL_STATE.gbfs_stations().await.clone();
        if !gbfs_stations.is_empty() {
            let res = sender.send(Message::Binary(gbfs_stations)).await;

            if let Err(e) = res {
                error!(?e, "Error sending initial GBFS stations");
                return Err(e);
            }
        }
    }

    Ok(())
}

async fn send_user_notices(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    user_id: &str,
) -> Result<(), axum::Error> {
    let notices = crate::admin::user_notices::for_user(user_id).await;
    if notices.is_empty() {
        return Ok(());
    }
    let versioned = Versioned::new(1, Broadcast::UserNotices(notices));
    let Ok(bytes) = minicbor_serde::to_vec(&versioned) else {
        return Ok(());
    };
    sender.send(Message::Binary(Bytes::from(bytes))).await
}
