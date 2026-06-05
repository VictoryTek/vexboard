mod api;
mod config;
mod db;
mod discovery;
mod metrics;
mod probe;
mod rate_limit;
mod session_store;

#[cfg(all(unix, feature = "pam-auth"))]
mod pam_auth;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::HeaderValue;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::SessionManagerLayer;

use crate::config::AppConfig;
use crate::discovery::DiscoveryList;
use crate::metrics::system::SystemSnapshot;

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<AppConfig>,
    pub discoveries: DiscoveryList,
    pub metrics_tx: broadcast::Sender<SystemSnapshot>,
    pub probe_tx: broadcast::Sender<probe::uptime::ProbeEvent>,
    pub login_limiter: Arc<rate_limit::LoginRateLimiter>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .init();

    tracing::info!("Starting VexBoard v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = AppConfig::load()?;
    let config = Arc::new(config);
    tracing::info!(host = %config.server.host, port = %config.server.port, "Configuration loaded");

    // Initialize database
    let db = db::init_pool(&config.database.path).await?;
    tracing::info!(path = %config.database.path.display(), "Database initialized");

    // Create broadcast channels
    let (metrics_tx, _) = broadcast::channel::<SystemSnapshot>(64);
    let (probe_tx, _) = broadcast::channel::<probe::uptime::ProbeEvent>(256);

    // Create discovery list
    let discoveries = discovery::new_discovery_list();

    // Build login rate limiter
    let login_limiter = Arc::new(rate_limit::LoginRateLimiter::new(
        config.auth.login_rate_limit_attempts,
        config.auth.login_rate_limit_window_secs,
    ));

    // Build application state
    let state = AppState {
        db: db.clone(),
        config: config.clone(),
        discoveries: discoveries.clone(),
        metrics_tx: metrics_tx.clone(),
        probe_tx: probe_tx.clone(),
        login_limiter,
    };

    // Spawn background tasks
    let disc_config = config.discovery.clone();
    let disc_db = db.clone();
    let disc_list = discoveries.clone();
    tokio::spawn(async move {
        discovery::systemd::discovery_loop(disc_list, disc_db, disc_config).await;
    });

    let docker_config = config.docker.clone();
    let docker_db = db.clone();
    let docker_list = discoveries.clone();
    tokio::spawn(async move {
        discovery::docker::docker_discovery_loop(docker_list, docker_db, docker_config).await;
    });

    let probe_config = config.probe.clone();
    let probe_db = db.clone();
    let probe_tx_clone = probe_tx.clone();
    tokio::spawn(async move {
        probe::start_probe_loop(probe_db, probe_config, probe_tx_clone).await;
    });

    let metrics_tx_clone = metrics_tx.clone();
    let metrics_interval = config.metrics.push_interval_ms;
    tokio::spawn(async move {
        metrics::system::metrics_loop(metrics_tx_clone, metrics_interval).await;
    });

    // Build router — use a SQLite-backed session store so sessions survive restarts.
    let session_store = session_store::SqliteSessionStore::new(db.clone());
    session_store.migrate().await?;
    let session_layer =
        SessionManagerLayer::new(session_store).with_secure(config.auth.secure_cookies);

    let app = api::router().with_state(state).layer(session_layer);

    // Serve static assets — fall back to index.html for any unmatched path so
    // that the Leptos client-side router handles routes like /setup and /login.
    let assets_root = if config.server.assets_path != "embedded" {
        config.server.assets_path.clone()
    } else {
        "assets".to_string()
    };
    let app = app.fallback_service(
        ServeDir::new(&assets_root).fallback(ServeFile::new(format!("{}/index.html", assets_root))),
    );

    let cors_layer = if config.server.allowed_origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<HeaderValue> = config
            .server
            .allowed_origins
            .iter()
            .filter_map(|o| match o.parse() {
                Ok(v) => Some(v),
                Err(_) => {
                    tracing::warn!(origin = %o, "Ignoring malformed CORS allowed_origin");
                    None
                }
            })
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    };
    let app = app.layer(cors_layer);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    tracing::info!(%addr, "Listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
