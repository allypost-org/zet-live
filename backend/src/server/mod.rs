use std::net::SocketAddr;

use listenfd::ListenFd;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

pub mod error;
pub mod request;
pub mod routes;

use crate::{
    admin, auth,
    cli::ServerConfig,
    database::Database,
    proto::{gbfs, gtfs_realtime, gtfs_schedule},
};

pub async fn run(server_config: &ServerConfig) -> anyhow::Result<()> {
    debug!("Starting server");

    if let Err(e) = Database::init(&server_config.database_url).await {
        error!(%e, "Failed to initialize database");
        return Err(anyhow::anyhow!(e).context("Failed to initialize database"));
    }

    auth::config::init(server_config).await;
    debug!(
        auth_enabled = crate::auth::config::get().enabled(),
        "Auth configured"
    );

    admin::init().await;

    auth::session::spawn_expiry_reaper();

    gtfs_realtime::fetcher::spawn_feed_fetcher();
    gtfs_schedule::fetcher::spawn_schedule_fetcher();
    gbfs::fetcher::spawn_all_feed_fetchers();

    info!("Waiting for initial schedule info and feed");
    {
        let settings = admin::ADMIN_SETTINGS.read().await;
        let realtime_paused = settings.realtime_paused.unwrap_or(false);
        let static_paused = settings.static_paused.unwrap_or(false);
        drop(settings);

        let mut js = tokio::task::JoinSet::new();

        if static_paused {
            debug!("Static schedule fetching paused, skipping initial wait");
        } else {
            js.spawn(async move {
                crate::proto::gtfs_schedule::fetcher::wait_for_schedule_update().await;
            });
        }

        if realtime_paused {
            debug!("Realtime fetching paused, skipping initial wait");
        } else {
            js.spawn(async move {
                crate::proto::gtfs_realtime::fetcher::wait_for_feed_update().await;
            });
        }

        while let Some(x) = js.join_next().await {
            x?;
        }
    }

    admin::run(server_config).await?;

    let app = routes::create_router(server_config.ip_source.clone())
        .into_make_service_with_connect_info::<SocketAddr>();

    let listener = create_listener(server_config).await?;
    info!(
        "Started listening on http://{}",
        listener.local_addr().expect("Failed to get local address")
    );

    if let Err(e) = axum::serve(listener, app).await {
        error!(?e, "Failed to start server");
        return Err(e.into());
    }

    Ok(())
}

async fn create_listener(server_config: &ServerConfig) -> anyhow::Result<TcpListener> {
    let mut listenfd = ListenFd::from_env();
    if let Ok(Some(listener)) = listenfd.take_tcp_listener(0) {
        debug!("Using socket from listenfd");
        return TcpListener::from_std(listener).map_err(Into::into);
    }

    match TcpListener::bind(server_config.bind_to).await {
        Ok(x) => Ok(x),
        Err(e) => {
            error!(?e, "Failed to bind to address");
            Err(anyhow::anyhow!(e).context(format!(
                "Failed to bind to address {:?}",
                server_config.bind_to
            )))
        }
    }
}
