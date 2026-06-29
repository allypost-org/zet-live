use cli::{CliCommands, Config};
use tracing::{debug, error, info, trace, warn};

use crate::config::project::ProjectConfig;

mod admin;
mod auth;
mod cli;
mod config;
mod database;
mod entity;
mod http_client;
mod logger;
mod proto;
mod server;

fn main() {
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(e) if e.not_found() => {}
        Err(e) => {
            eprintln!("Failed to load .env file: {}", e);
        }
    }
    logger::init();

    let config = match Config::init() {
        Err(err) => {
            error!(?err, "Failed to initialize CLI args");
            std::process::exit(1);
        }
        Ok(x) => x,
    };

    if let Some(log_level) = config.global.log_level.as_ref()
        && let Err(e) = logger::update_log_level(log_level)
    {
        error!(?e, "Failed to update log level");
    }

    info!(version = %ProjectConfig::app_version(), "Running zet-live");

    debug!(
        app = ProjectConfig::app_name_with_version(),
        built = ProjectConfig::build_date(),
        rustc_version = ProjectConfig::rustc_version(),
        "Build info"
    );

    trace!(?config, "CLI args initialized");

    match config.cmd {
        CliCommands::Server(ref server_config) => {
            let token = tokio_util::sync::CancellationToken::new();
            #[cfg(not(target_os = "windows"))]
            {
                let token = token.clone();
                let mut signals =
                    signal_hook::iterator::Signals::new(signal_hook::consts::TERM_SIGNALS)
                        .expect("Failed to create signals");

                std::thread::spawn(move || {
                    if let Some(signal) = signals.forever().next() {
                        warn!("Received signal {signal}, shutting down");
                        token.cancel();
                        let secs = 8;
                        std::thread::sleep(std::time::Duration::from_secs(secs));
                        error!(
                            "Could not shut down gracefully after {secs} seconds, forcefully \
                             shutting down"
                        );
                        std::process::exit(1);
                    }
                });
            }

            let runtime = match tokio::runtime::Runtime::new() {
                Ok(x) => x,
                Err(err) => {
                    error!(?err, "Failed to create tokio runtime");
                    std::process::exit(1);
                }
            };

            let res = token.run_until_cancelled(server::run(server_config));
            let res = runtime.block_on(res);

            runtime.shutdown_timeout(std::time::Duration::from_secs(1));
            match res {
                Some(Ok(())) | None => {
                    info!("Server stopped successfully");
                    std::process::exit(0);
                }
                Some(Err(e)) => {
                    error!(?e, "Server stopped with error");
                    std::process::exit(1);
                }
            }
        }
    }
}
