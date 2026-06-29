use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use axum_client_ip::ClientIpSource;
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;
use validator::ValidationError;

static CLI_ARGS: OnceLock<Arc<Config>> = OnceLock::new();

#[derive(Debug, clap::Parser)]
#[clap(version)]
pub struct Config {
    #[clap(flatten)]
    pub global: GlobalConfig,

    #[clap(subcommand)]
    pub cmd: CliCommands,
}

impl Config {
    pub fn init() -> anyhow::Result<Arc<Self>> {
        CLI_ARGS
            .set(Arc::new(Self::parse()))
            .map(|()| Self::global())
            .map_err(|_| anyhow::anyhow!("Failed to initialize CLI args"))
    }

    pub fn global() -> Arc<Self> {
        CLI_ARGS.get().expect("CLI args not initialized").clone()
    }
}

#[derive(Debug, clap::Args)]
pub struct GlobalConfig {
    /// Dump shell completions to stdout
    #[arg(
        long,
        default_value = None,
        value_name = "SHELL",
        value_parser = hacky_dump_completions()
    )]
    pub dump_completions: Option<Shell>,

    /// The log level to use.
    ///
    /// Can be a comma-separated list of loggers and their levels.
    /// Eg. `zet_live=trace,request=debug,warn`
    #[clap(long, env = "LOG_LEVEL")]
    pub log_level: Option<String>,

    #[clap(flatten)]
    pub data_fetcher: DataFetcherConfig,
}

#[derive(Debug, clap::Args)]
pub struct DataFetcherConfig {
    /// The endpoint to fetch the data from.
    /// Must be a valid URL to a GTFS Realtime endpoint.
    ///
    /// @see <https://gtfs.org/documentation/realtime/reference/>
    #[clap(
        long,
        default_value = "https://www.zet.hr/gtfs-rt-protobuf",
        env = "ZI_DATA_FETCH_ENDPOINT"
    )]
    pub data_fetch_endpoint: url::Url,

    /// The interval at which the data is fetched/checked from the endpoint.
    /// Depends on the endpoint, but for the ZET GTFS-RT endpoint, it's updated about every 10 seconds.
    ///
    /// Accepts a duration in human-friendly format or ISO 8601.
    /// Eg. 2 seconds, 2 minutes, 1d, 3 months, PT2S, 4h, 5mins, 6s
    #[clap(
        long,
        value_parser = parse_span,
        default_value = "2 seconds",
        env = "ZI_DATA_FETCH_INTERVAL"
    )]
    pub data_fetch_interval: jiff::Span,

    /// The endpoint to fetch the schedule from.
    ///
    /// Should be a link to a zip file containing CSV files with schedule data about the GTFS feed.
    ///
    /// @see <https://gtfs.org/documentation/schedule/reference/>
    #[clap(
        long,
        default_value = "https://www.zet.hr/gtfs-scheduled/latest",
        env = "ZI_SCHEDULE_FETCH_ENDPOINT"
    )]
    pub schedule_fetch_endpoint: url::Url,

    /// The interval at which the data is fetched/checked from the endpoint.
    /// Depends on the endpoint, but for the ZET GTFS-RT endpoint, it's updated about every 10 seconds.
    ///
    /// Accepts a duration in human-friendly format or ISO 8601.
    /// Eg. 2 seconds, 2 minutes, 1d, 3 months, PT2S, 4h, 5mins, 6s
    #[clap(
        long,
        value_parser = parse_span,
        default_value = "2 minutes",
        env = "ZI_SCHEDULE_FETCH_INTERVAL"
    )]
    pub schedule_fetch_interval: jiff::Span,

    /// The GBFS auto-discovery endpoint (`gbfs.json`) to fetch bike-share data from.
    ///
    /// @see <https://gbfs.org/documentation/gbfs/v2.3>
    #[clap(
        long,
        default_value = "https://gbfs.nextbike.net/maps/gbfs/v2/nextbike_hd/gbfs.json",
        env = "GBFS_FETCH_ENDPOINT"
    )]
    pub gbfs_fetch_endpoint: url::Url,

    /// The language code to use when resolving GBFS feed URLs from `gbfs.json`
    /// (e.g. `en`, `hr`, `de`). Falls back to the first available language if
    /// the requested one is not present.
    #[clap(long, default_value = "en", env = "GBFS_LANGUAGE")]
    pub gbfs_language: String,

    /// The minimum interval between GBFS fetches for a single feed. Each feed
    /// otherwise polls at half its advertised TTL (plus jitter); this acts as a
    /// floor to avoid hammering feeds that advertise a very small TTL.
    ///
    /// Accepts a duration in human-friendly format or ISO 8601.
    /// Eg. 2 seconds, 2 minutes, 1d, 3 months, PT2S, 4h, 5mins, 6s
    #[clap(
        long,
        value_parser = parse_span,
        default_value = "5 seconds",
        env = "GBFS_MIN_FETCH_INTERVAL"
    )]
    pub gbfs_min_fetch_interval: jiff::Span,
}

#[derive(Debug, clap::Subcommand)]
pub enum CliCommands {
    Server(ServerConfig),
}

#[derive(Debug, clap::Args)]
pub struct ServerConfig {
    /// The address to bind the server to.
    ///
    /// Should usually be either `0.0.0.0:$PORT` if you want to bind to all interfaces aka the public,
    /// or `127.0.0.1:$PORT` if you don't want to expose the server to the outside world.
    ///
    /// You can set the port to 0 to have the server pick a random available port.
    #[clap(long, env = "BIND_TO", default_value = "0.0.0.0:9011")]
    pub bind_to: SocketAddr,

    /// The `SQLite` database URL to use.
    ///
    /// Should be a valid database URL, such as `sqlite:./db.sqlite`.
    #[clap(long, default_value = ":memory:", env = "DATABASE_URL", value_parser = DatabaseUrl::try_from_string)]
    pub database_url: DatabaseUrl,

    /// The source to use for the client's IP address.
    ///
    /// This is used for logging and rate limiting.
    /// Should be set to `rightmost-x-forwarded-for` if the server is behind a reverse proxy
    #[clap(long, env = "IP_SOURCE", default_value_t = ClientIpSource::RightmostXForwardedFor)]
    pub ip_source: ClientIpSource,

    /// The bearer token required to access the admin API.
    ///
    /// If not set, the admin server will not be started.
    /// Also requires admin-bind-to to be set.
    #[clap(long, env = "ADMIN_KEY")]
    pub admin_key: Option<String>,

    /// The address to bind the admin server to.
    ///
    /// If not set, the admin server will not be started.
    /// Also requires admin-key to be set.
    ///
    /// Should usually be either `0.0.0.0:$PORT` if you want to bind to all interfaces aka the public,
    /// or `127.0.0.1:$PORT` if you don't want to expose the server to the outside world.
    ///
    /// You can set the port to 0 to have the server pick a random available port.
    #[clap(long, env = "ADMIN_BIND_TO")]
    pub admin_bind_to: Option<SocketAddr>,

    /// The public base URL of the application, used to build `OAuth2` redirect
    /// URIs (e.g. `https://zet-live.example.com`). Required when any auth
    /// provider is enabled in the admin page; if absent, user accounts are
    /// disabled entirely.
    #[clap(long, env = "APP_URL")]
    pub app_url: Option<url::Url>,

    /// Comma-separated list of extra frontend origins allowed as the OAuth
    /// popup `postMessage` target (which carries the session token). The
    /// `APP_URL` origin is always allowed automatically; this is only needed
    /// for split-origin setups (e.g. Vite on `http://localhost:5173` while the
    /// backend is on `:9011`).
    #[clap(long, env = "ALLOWED_FRONTEND_ORIGINS", value_delimiter = ',')]
    pub allowed_frontend_origins: Vec<String>,

    /// Session token lifetime in seconds.
    #[clap(long, env = "SESSION_MAX_AGE", default_value = "30 days")]
    pub session_max_age: jiff::Span,
}

#[derive(Debug, Clone)]
pub enum DatabaseUrl {
    Memory,
    Local(PathBuf),
}
impl DatabaseUrl {
    fn try_from_string(s: &str) -> Result<Self, String> {
        if s == ":memory:" {
            return Ok(Self::Memory);
        }

        url::Url::parse(s).map_or_else(
            |_| Ok(Self::Local(PathBuf::from(s))),
            |url| match url.scheme() {
                "sqlite" | "sqlite3" | "file" => Ok(Self::Local(PathBuf::from(url.path()))),
                _ => {
                    Err("Invalid database URL scheme (expected sqlite:, sqlite3:, or file:)".into())
                }
            },
        )
    }
}
impl std::fmt::Display for DatabaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, ":memory:"),
            Self::Local(path) => write!(f, "file://{}", path.display()),
        }
    }
}

#[must_use]
fn hacky_dump_completions() -> impl clap::builder::TypedValueParser {
    move |s: &str| {
        let parsed = Shell::from_str(s, true);

        if let Ok(shell) = &parsed {
            let bin_name = std::env::current_exe()
                .map_err(|_e| ValidationError::new("Unknown application name"))?
                .file_name()
                .map(|x| x.to_string_lossy().to_string())
                .ok_or_else(|| ValidationError::new("Unknown application name"))?;

            clap_complete::generate(
                *shell,
                &mut Config::command(),
                bin_name,
                &mut std::io::stdout(),
            );
            std::process::exit(0);
        }

        parsed
            .map(|_| ())
            .map_err(|_| ValidationError::new("Invalid shell"))
    }
}

fn parse_span(arg: &str) -> Result<jiff::Span, String> {
    arg.parse::<jiff::Span>().map_err(|e| e.to_string())
}
