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
use tokio::{
    sync::{RwLock, broadcast},
    time,
};
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
    let mut flag_rx = state.flag_changes.subscribe();
    // Subscribe before the initial snapshot. A receiver only sees transmissions
    // published after it subscribes, so subscribing later would lose anything
    // sent while the snapshot frames were being written. Those later frames can
    // repeat a snapshot entry, which is harmless because every kind replaces
    // the client's whole state.
    let mut transmission_rx = state.get_transmission_receiver();

    if !send_full_state(None, &mut sender).await {
        cleanup_connection(addr).await;
        return;
    }
    if !send_feature_flags(user_id.as_deref(), &mut sender).await {
        cleanup_connection(addr).await;
        return;
    }

    let mut ping_interval = {
        #[allow(clippy::cast_sign_loss)]
        let interval = (30_000 + rand::random_range(-5_000_i64..5_000)) as u64;
        time::interval(Duration::from_millis(interval))
    };
    let mut notification_rx = get_admin_notification_receiver();

    loop {
        tokio::select! {
            result = flag_rx.changed() => {
                if result.is_err() {
                    break;
                }
                flag_rx.borrow_and_update();
                if !send_feature_flags(user_id.as_deref(), &mut sender).await {
                    break;
                }
            }
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
            result = transmission_rx.recv() => {
                let transmission = match result {
                    Ok(t) => t,
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(count, "Transmission channel lagged, resending full state");
                        drain_transmissions(&mut transmission_rx);
                        if !send_full_state(user_id.as_deref(), &mut sender).await {
                            break;
                        }
                        continue;
                    }
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
                            if !send_feature_flags(user_id.as_deref(), &mut sender).await {
                                break;
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
    }
}

/// Discard the transmissions queued for a lagging connection, before the caller
/// resends the full snapshot. Every broadcast replaces its `INITIAL_STATE` entry
/// before it is published, so a transmission queued before the snapshot is read
/// is already reflected in it. Draining first therefore never drops a newer
/// value, and keeps an older frame from arriving after the snapshot and moving
/// the client backwards. A producer that outruns the drain stops it early by
/// lagging again, which the next receive detects and recovers from.
fn drain_transmissions(rx: &mut broadcast::Receiver<Arc<Transmission>>) {
    while rx.try_recv().is_ok() {}
}

async fn send_feature_flags(
    user_id: Option<&str>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let versioned = Versioned::new(
        1,
        Broadcast::FeatureFlags(crate::feature_flags::enabled_map(user_id)),
    );
    let Ok(bytes) = minicbor_serde::to_vec(&versioned) else {
        warn!("Failed to serialize feature-flags broadcast");
        return false;
    };
    sender
        .send(Message::Binary(Bytes::from(bytes)))
        .await
        .is_ok()
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

/// Push every current snapshot entry plus the account's notices, replacing
/// whatever the client already holds. Used when a connection opens and to
/// recover one that lagged behind and missed transmissions.
async fn send_full_state(
    user_id: Option<&str>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let snapshots = [
        INITIAL_STATE.vehicles().await.clone(),
        INITIAL_STATE.active_stops().await.clone(),
        INITIAL_STATE.notices().await.clone(),
        INITIAL_STATE.gbfs_stations().await.clone(),
        INITIAL_STATE.simple_stops().await.clone(),
    ];

    for snapshot in snapshots {
        if snapshot.is_empty() {
            continue;
        }

        if sender.send(Message::Binary(snapshot)).await.is_err() {
            error!("Error sending full state");
            return false;
        }
    }

    if let Some(user_id) = user_id
        && send_user_notices(sender, user_id).await.is_err()
    {
        error!(%user_id, "Error sending user notices with full state");
        return false;
    }

    true
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
