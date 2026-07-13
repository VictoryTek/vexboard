mod api;
mod config;
mod db;
mod discovery;
mod metrics;
mod middleware;
mod notify;
mod probe;
mod rate_limit;
mod session_store;

#[cfg(test)]
mod tests;

#[cfg(all(unix, feature = "pam-auth"))]
mod pam_auth;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::SessionManagerLayer;

use crate::config::AppConfig;
use crate::discovery::DiscoveryList;
use crate::metrics::system::SystemSnapshot;

/// Returns true if the last path segment contains a `.` (looks like an asset
/// request rather than a client-side route).
fn has_extension(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|f| f.contains('.'))
}

/// Returns true if the filename looks like a Trunk content-hashed asset, i.e.
/// `<name>-<hash>.<ext>` or `<name>-<hash>_bg.wasm`, where `<hash>` is at least
/// 8 lowercase hex characters. Such filenames are safe to cache forever since
/// any content change produces a new filename.
fn is_hashed_asset(path: &str) -> bool {
    let Some(file_name) = path.rsplit('/').next().filter(|f| !f.is_empty()) else {
        return false;
    };
    let stem = file_name.split('.').next().unwrap_or(file_name);
    let stem = stem.strip_suffix("_bg").unwrap_or(stem);
    match stem.rsplit_once('-') {
        Some((_, hash)) => hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// Serves the frontend bundle from `assets_root`, distinguishing between
/// genuinely missing assets (404), client-side routes (served `index.html`,
/// never cached without revalidation), and content-hashed build artifacts
/// (cached forever, since their filename changes whenever their content does).
async fn spa_asset_service(
    assets_root: String,
    req: Request<Body>,
) -> Result<Response, Infallible> {
    let path = req.uri().path().to_string();

    let resp = ServeDir::new(&assets_root)
        .oneshot(req)
        .await
        .expect("ServeDir is infallible")
        .map(Body::new);

    if resp.status() == StatusCode::NOT_FOUND {
        if has_extension(&path) {
            // Genuinely missing asset — preserve the 404.
            return Ok(resp);
        }

        let index_req = Request::builder()
            .method("GET")
            .uri("/index.html")
            .body(Body::empty())
            .expect("static index.html request is well-formed");
        let mut fallback = ServeFile::new(format!("{}/index.html", assets_root))
            .oneshot(index_req)
            .await
            .expect("ServeFile is infallible")
            .map(Body::new);
        fallback.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, must-revalidate"),
        );
        return Ok(fallback);
    }

    let mut resp = resp;
    let cache_control = if is_hashed_asset(&path) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache, must-revalidate"
    };
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    Ok(resp)
}

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<AppConfig>,
    pub discoveries: DiscoveryList,
    pub metrics_tx: broadcast::Sender<SystemSnapshot>,
    pub probe_tx: broadcast::Sender<probe::uptime::ProbeEvent>,
    pub probe_client: reqwest::Client,
    pub probe_client_insecure: reqwest::Client,
    pub login_limiter: Arc<rate_limit::LoginRateLimiter>,
    pub session_store: session_store::SqliteSessionStore,
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
    let mut config = AppConfig::load()?;
    tracing::info!(host = %config.server.host, port = %config.server.port, "Configuration loaded");

    // Initialize database
    let db = db::init_pool(&config.database.path).await?;
    tracing::info!(path = %config.database.path.display(), "Database initialized");

    // A DB-stored auth mode (set via the Settings page) overrides the file/env
    // config at startup — this is the only writable override path on deployments
    // (e.g. the NixOS module) where /etc/vexboard/config.toml is read-only.
    match db::get_setting(&db, "auth_mode").await {
        Ok(Some(stored)) if stored == "session" || stored == "none" => {
            if stored != config.auth.mode {
                tracing::info!(mode = %stored, "Applying auth mode override from settings");
                config.auth.mode = stored;
            }
        }
        Ok(Some(other)) => {
            tracing::warn!(value = %other, "Ignoring invalid stored auth_mode setting");
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("Failed to read auth_mode setting: {e}");
        }
    }
    if config.auth.mode == "session" && config.auth.secret.len() < 32 {
        anyhow::bail!(
            "auth.secret must be at least 32 bytes (got {}); generate one with `openssl rand -base64 48`",
            config.auth.secret.len()
        );
    }
    let config = Arc::new(config);

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

    // SQLite-backed session store so sessions survive restarts.
    let session_store = session_store::SqliteSessionStore::new(db.clone());
    session_store.migrate().await?;

    let cleanup_store = session_store.clone();
    tokio::spawn(async move {
        session_store::session_cleanup_loop(cleanup_store, std::time::Duration::from_secs(3600))
            .await;
    });

    // Shared HTTP client for uptime probes — reused across every probe instead of
    // building a fresh connection pool/TLS config per request.
    let probe_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.probe.timeout_secs))
        .danger_accept_invalid_certs(false)
        .build()?;

    // Second shared client for services explicitly opted into skipping TLS
    // verification (e.g. self-signed certs like Proxmox VE's default cert).
    // Only used for services with `skip_tls_verify = true`.
    let probe_client_insecure = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.probe.timeout_secs))
        .danger_accept_invalid_certs(true)
        .build()?;

    // Build application state
    let state = AppState {
        db: db.clone(),
        config: config.clone(),
        discoveries: discoveries.clone(),
        metrics_tx: metrics_tx.clone(),
        probe_tx: probe_tx.clone(),
        probe_client: probe_client.clone(),
        probe_client_insecure: probe_client_insecure.clone(),
        login_limiter,
        session_store: session_store.clone(),
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
    let probe_loop_client = probe_client.clone();
    let probe_loop_client_insecure = probe_client_insecure.clone();
    tokio::spawn(async move {
        probe::start_probe_loop(
            probe_db,
            probe_config,
            probe_tx_clone,
            probe_loop_client,
            probe_loop_client_insecure,
        )
        .await;
    });

    let metrics_tx_clone = metrics_tx.clone();
    let metrics_interval = config.metrics.push_interval_ms;
    tokio::spawn(async move {
        metrics::system::metrics_loop(metrics_tx_clone, metrics_interval).await;
    });

    let notify_config = config.notifications.clone();
    let notify_rx = probe_tx.subscribe();
    let notify_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    tokio::spawn(async move {
        notify::notification_loop(notify_rx, notify_config, notify_client).await;
    });

    // Build router.
    // `AppConfig::load()` only enforces the 32-byte minimum when auth.mode == "session";
    // `auth.mode == "none"` deployments never exercise login, so fall back to a random,
    // ephemeral key rather than panicking on a short/default secret.
    let session_key = if config.auth.secret.len() >= 32 {
        tower_sessions::cookie::Key::derive_from(config.auth.secret.as_bytes())
    } else {
        tower_sessions::cookie::Key::generate()
    };
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(config.auth.secure_cookies)
        .with_expiry(tower_sessions::Expiry::OnInactivity(
            time::Duration::seconds(config.auth.session_ttl_hours as i64 * 3600),
        ))
        .with_signed(session_key);

    if config.auth.mode == "none" {
        tracing::warn!(
            "auth.mode = \"none\": all API routes are unauthenticated; only use this if the network layer restricts access"
        );
    }
    let app = api::router(&config.auth.mode)
        .with_state(state)
        .layer(session_layer);

    // Serve static assets — fall back to index.html for any unmatched path so
    // that the Leptos client-side router handles routes like /setup and /login.
    let assets_root = if config.server.assets_path != "embedded" {
        config.server.assets_path.clone()
    } else {
        "assets".to_string()
    };
    let app = app.fallback_service(tower::service_fn(move |req| {
        spa_asset_service(assets_root.clone(), req)
    }));

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
