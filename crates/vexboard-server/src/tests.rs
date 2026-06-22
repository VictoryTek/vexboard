use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sqlx::SqlitePool;
use tower::ServiceExt;
use tower_sessions::{MemoryStore, SessionManagerLayer};

use crate::config::{
    AppConfig, AuthConfig, DatabaseConfig, DiscoveryConfig, DockerConfig, MetricsConfig,
    NotificationsConfig, ProbeConfig, ServerConfig,
};
use crate::rate_limit::LoginRateLimiter;
use crate::{discovery, AppState};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

fn test_config() -> AppConfig {
    AppConfig {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            assets_path: "assets".to_string(),
            allowed_origins: vec!["*".to_string()],
            icon_cdn_base: "https://cdn.jsdelivr.net/gh/selfhst/icons@main".to_string(),
        },
        database: DatabaseConfig {
            path: PathBuf::from(":memory:"),
        },
        auth: AuthConfig {
            secret: "test-secret".to_string(),
            session_ttl_hours: 24,
            secure_cookies: false,
            login_rate_limit_attempts: 0,
            login_rate_limit_window_secs: 60,
        },
        discovery: DiscoveryConfig {
            enabled: false,
            interval_secs: 60,
            exclude_units: vec![],
            server_services_only: false,
        },
        docker: DockerConfig {
            enabled: false,
            interval_secs: 60,
            sockets: vec![],
            exclude_images: vec![],
        },
        probe: ProbeConfig {
            default_interval_secs: 30,
            timeout_secs: 5,
            max_history: 100,
        },
        metrics: MetricsConfig {
            push_interval_ms: 1000,
        },
        notifications: NotificationsConfig::default(),
    }
}

struct TestApp {
    pool: SqlitePool,
    app: Router,
}

impl TestApp {
    async fn new() -> Self {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        let (metrics_tx, _) = tokio::sync::broadcast::channel(4);
        let (probe_tx, _) = tokio::sync::broadcast::channel(4);

        let state = AppState {
            db: pool.clone(),
            config: Arc::new(test_config()),
            discoveries: discovery::new_discovery_list(),
            metrics_tx,
            probe_tx,
            login_limiter: Arc::new(LoginRateLimiter::new(0, 60)),
        };

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
        let app = crate::api::router().with_state(state).layer(session_layer);

        TestApp { pool, app }
    }

    /// Insert an admin user directly (bcrypt cost 4 for speed).
    async fn seed_admin(&self, username: &str, password: &str) {
        let hash = bcrypt::hash(password, 4).unwrap();
        sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, 'admin')")
            .bind(username)
            .bind(hash)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// Insert a viewer user directly.
    async fn seed_viewer(&self, username: &str, password: &str) {
        let hash = bcrypt::hash(password, 4).unwrap();
        sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, 'viewer')")
            .bind(username)
            .bind(hash)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// POST /api/v1/auth/login and return (status, session_cookie).
    async fn login(&self, username: &str, password: &str) -> (StatusCode, String) {
        let body = serde_json::json!({"username": username, "password": password}).to_string();
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo("127.0.0.1:0".parse::<SocketAddr>().unwrap()));

        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let cookie = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or("").to_string())
            .unwrap_or_default();
        (status, cookie)
    }

    async fn get_json(&self, uri: &str, cookie: &str) -> (StatusCode, Value) {
        let mut builder = Request::builder().method("GET").uri(uri);
        if !cookie.is_empty() {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::empty()).unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    async fn post_json(&self, uri: &str, payload: Value, cookie: &str) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if !cookie.is_empty() {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::from(payload.to_string())).unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    async fn delete_req(&self, uri: &str, cookie: &str) -> (StatusCode, Value) {
        let mut builder = Request::builder().method("DELETE").uri(uri);
        if !cookie.is_empty() {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::empty()).unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    async fn post_empty(&self, uri: &str, cookie: &str) -> (StatusCode, Value) {
        let mut builder = Request::builder().method("POST").uri(uri);
        if !cookie.is_empty() {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::empty()).unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_check() {
    let app = TestApp::new().await;
    let (status, _) = app.get_json("/health", "").await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Auth — login
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_login_success() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;

    let (status, cookie) = app.login("admin", "password123").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!cookie.is_empty(), "session cookie should be set");
}

#[tokio::test]
async fn test_login_wrong_password() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;

    let (status, _) = app.login("admin", "wrongpassword").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_unknown_user() {
    let app = TestApp::new().await;

    let (status, _) = app.login("nobody", "password123").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Auth — /me
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_me_unauthenticated() {
    let app = TestApp::new().await;
    let (status, _) = app.get_json("/api/v1/auth/me", "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_me_authenticated_returns_username_and_role() {
    let app = TestApp::new().await;
    app.seed_admin("alice", "password123").await;

    let (login_status, cookie) = app.login("alice", "password123").await;
    assert_eq!(login_status, StatusCode::OK);

    let (status, body) = app.get_json("/api/v1/auth/me", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "alice");
    assert_eq!(body["user"]["role"], "admin");
}

// ---------------------------------------------------------------------------
// Auth — logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_logout_invalidates_session() {
    let app = TestApp::new().await;
    app.seed_admin("bob", "password123").await;

    let (_, cookie) = app.login("bob", "password123").await;
    assert!(!cookie.is_empty());

    // Confirm authenticated before logout
    let (me_before, _) = app.get_json("/api/v1/auth/me", &cookie).await;
    assert_eq!(me_before, StatusCode::OK);

    // Logout
    let (logout_status, _) = app.post_empty("/api/v1/auth/logout", &cookie).await;
    assert_eq!(logout_status, StatusCode::OK);

    // Session should now be invalidated
    let (me_after, _) = app.get_json("/api/v1/auth/me", &cookie).await;
    assert_eq!(me_after, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Middleware enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_services_unauthenticated_returns_401() {
    let app = TestApp::new().await;
    let (status, _) = app.get_json("/api/v1/services", "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_route_as_viewer_returns_403() {
    let app = TestApp::new().await;
    app.seed_viewer("viewer", "password123").await;

    let (_, cookie) = app.login("viewer", "password123").await;
    assert!(!cookie.is_empty());

    let payload = serde_json::json!({
        "display_name": "Test Service",
        "probe_enabled": false
    });
    let (status, _) = app.post_json("/api/v1/services", payload, &cookie).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Services CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_services_returns_empty_array() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;

    let (_, cookie) = app.login("admin", "password123").await;

    let (status, body) = app.get_json("/api/v1/services", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_service_as_admin() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;

    let (_, cookie) = app.login("admin", "password123").await;

    let payload = serde_json::json!({
        "display_name": "My Service",
        "url": "http://localhost:8080",
        "probe_enabled": false
    });
    let (status, body) = app.post_json("/api/v1/services", payload, &cookie).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(
        body["id"].as_i64().is_some(),
        "response should include new id"
    );
}

#[tokio::test]
async fn test_create_and_delete_service_as_admin() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;

    let (_, cookie) = app.login("admin", "password123").await;

    // Create
    let payload = serde_json::json!({
        "display_name": "Temporary Service",
        "probe_enabled": false
    });
    let (create_status, create_body) = app.post_json("/api/v1/services", payload, &cookie).await;
    assert_eq!(create_status, StatusCode::CREATED);
    let id = create_body["id"].as_i64().unwrap();

    // Delete
    let (delete_status, _) = app
        .delete_req(&format!("/api/v1/services/{id}"), &cookie)
        .await;
    assert_eq!(delete_status, StatusCode::OK);

    // Verify gone
    let (list_status, list_body) = app.get_json("/api/v1/services", &cookie).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_service_as_viewer_returns_403() {
    let app = TestApp::new().await;
    app.seed_viewer("viewer", "password123").await;

    let (_, cookie) = app.login("viewer", "password123").await;

    let payload = serde_json::json!({
        "display_name": "Viewer Service",
        "probe_enabled": false
    });
    let (status, _) = app.post_json("/api/v1/services", payload, &cookie).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
