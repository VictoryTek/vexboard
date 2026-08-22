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
use tower_sessions::SessionManagerLayer;

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
            mode: "session".to_string(),
            behind_proxy: false,
            pam_admin_users: vec![],
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
            history_retention_days: 30,
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
        Self::new_with_auth_mode("session").await
    }

    /// Same as `new()`, but with `auth.mode` set on both the state config and
    /// the router build — mirrors how `main.rs` keeps the two in sync at startup.
    async fn new_with_auth_mode(mode: &str) -> Self {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        let (metrics_tx, _) = tokio::sync::broadcast::channel(4);
        let (probe_tx, _) = tokio::sync::broadcast::channel(4);

        let session_store = crate::session_store::SqliteSessionStore::new(pool.clone());
        session_store.migrate().await.unwrap();

        let mut config = test_config();
        config.auth.mode = mode.to_string();

        let state = AppState {
            db: pool.clone(),
            config: Arc::new(config),
            discoveries: discovery::new_discovery_list(),
            metrics_tx,
            probe_tx,
            probe_client: reqwest::Client::builder()
                .tls_certs_only(std::iter::empty())
                .build()
                .unwrap(),
            probe_client_insecure: reqwest::Client::builder()
                .tls_certs_only(std::iter::empty())
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap(),
            login_limiter: Arc::new(LoginRateLimiter::new(0, 60)),
            session_store: session_store.clone(),
        };

        let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
        let app = crate::api::router(mode, state.clone())
            .with_state(state)
            .layer(session_layer);

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

    async fn get_text(&self, uri: &str, cookie: &str) -> (StatusCode, String) {
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
        (status, String::from_utf8_lossy(&bytes).to_string())
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

    async fn put_json(&self, uri: &str, payload: Value, cookie: &str) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method("PUT")
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
// db::try_claim_setting — atomic one-time flag used for PAM bootstrap admin
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_try_claim_setting_first_caller_wins() {
    let app = TestApp::new().await;

    let claimed = crate::db::try_claim_setting(&app.pool, "pam_bootstrap_admin", "alice")
        .await
        .unwrap();
    assert!(claimed, "first claim should succeed");

    let value = crate::db::get_setting(&app.pool, "pam_bootstrap_admin")
        .await
        .unwrap();
    assert_eq!(value.as_deref(), Some("alice"));
}

#[tokio::test]
async fn test_try_claim_setting_second_caller_loses() {
    let app = TestApp::new().await;

    let first = crate::db::try_claim_setting(&app.pool, "pam_bootstrap_admin", "alice")
        .await
        .unwrap();
    assert!(first);

    let second = crate::db::try_claim_setting(&app.pool, "pam_bootstrap_admin", "bob")
        .await
        .unwrap();
    assert!(!second, "second claim should lose to the first");

    // Value stays whatever the first (winning) caller wrote.
    let value = crate::db::get_setting(&app.pool, "pam_bootstrap_admin")
        .await
        .unwrap();
    assert_eq!(value.as_deref(), Some("alice"));
}

/// Regression: the bootstrap admin must stay admin across repeat logins.
///
/// The role was decided from `try_claim_setting`'s bool, which goes `false` once the
/// row exists — including for the user who claimed it. The PAM bootstrap admin was
/// therefore granted admin on their first login and demoted to viewer on every login
/// after, locked out of the instance they had just set up.
#[tokio::test]
async fn test_bootstrap_admin_stays_admin_on_repeat_logins() {
    use crate::db::BootstrapAdmin;
    let app = TestApp::new().await;

    let first = crate::db::claim_bootstrap_admin(&app.pool, "alice")
        .await
        .unwrap();
    assert_eq!(first, BootstrapAdmin::Granted);

    // Same user logging in again — previously this returned the "lost the claim"
    // path and dropped her to viewer.
    for _ in 0..3 {
        let again = crate::db::claim_bootstrap_admin(&app.pool, "alice")
            .await
            .unwrap();
        assert_eq!(
            again,
            BootstrapAdmin::AlreadyHeld,
            "the holder of the claim must remain admin on subsequent logins"
        );
    }
}

/// The grant stays one-time: nobody else can inherit it.
#[tokio::test]
async fn test_bootstrap_admin_denied_to_other_users() {
    use crate::db::BootstrapAdmin;
    let app = TestApp::new().await;

    crate::db::claim_bootstrap_admin(&app.pool, "alice")
        .await
        .unwrap();

    let bob = crate::db::claim_bootstrap_admin(&app.pool, "bob")
        .await
        .unwrap();
    assert_eq!(bob, BootstrapAdmin::HeldByOther);

    // And the claim still belongs to alice.
    let value = crate::db::get_setting(&app.pool, "pam_bootstrap_admin")
        .await
        .unwrap();
    assert_eq!(value.as_deref(), Some("alice"));
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

#[tokio::test]
async fn test_me_returns_ok_with_no_session_when_auth_mode_none() {
    let app = TestApp::new_with_auth_mode("none").await;
    let (status, body) = app.get_json("/api/v1/auth/me", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["auth_mode"], "none");
    assert_eq!(body["user"]["role"], "admin");
}

/// Regression: with Disable Login on and no session, a single-account instance
/// should resolve to that real account (identity + saved sort preference)
/// instead of a synthetic "anonymous" identity — there's no ambiguity about
/// who's at the keyboard when there's exactly one user.
#[tokio::test]
async fn test_me_auth_mode_none_resolves_sole_user() {
    let app = TestApp::new_with_auth_mode("none").await;
    app.seed_admin("alice", "password123").await;
    app.put_json(
        "/api/v1/auth/me/sort-mode",
        serde_json::json!({"sort_mode": "group"}),
        "",
    )
    .await;

    let (status, body) = app.get_json("/api/v1/auth/me", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "alice");
    assert_eq!(body["user"]["role"], "admin");
    assert_eq!(body["user"]["auth_mode"], "none");
    assert_eq!(body["user"]["dashboard_sort_mode"], "group");
}

/// Ambiguous case: with Disable Login on and no session, two accounts on the
/// instance means the server can't guess which one is the real caller — falls
/// back to the existing synthetic anonymous identity unchanged.
#[tokio::test]
async fn test_me_auth_mode_none_falls_back_to_anonymous_with_multiple_users() {
    let app = TestApp::new_with_auth_mode("none").await;
    app.seed_admin("alice", "password123").await;
    app.seed_viewer("bob", "password123").await;

    let (status, body) = app.get_json("/api/v1/auth/me", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "anonymous");
    assert_eq!(body["user"]["role"], "admin");
    assert_eq!(body["user"]["dashboard_sort_mode"], "az");
}

/// Regression: with Disable Login on and an ambiguous account count (here,
/// two accounts), the sort preference must still persist across requests
/// instead of silently discarding the write (previously a hardcoded "az" on
/// every GET and a 401 on every PUT, since there was no resolvable identity
/// to key the setting under).
#[tokio::test]
async fn test_sort_mode_persists_with_ambiguous_account_count() {
    let app = TestApp::new_with_auth_mode("none").await;
    app.seed_admin("alice", "password123").await;
    app.seed_viewer("bob", "password123").await;

    let (put_status, _) = app
        .put_json(
            "/api/v1/auth/me/sort-mode",
            serde_json::json!({"sort_mode": "group"}),
            "",
        )
        .await;
    assert_eq!(put_status, StatusCode::OK);

    let (status, body) = app.get_json("/api/v1/auth/me", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "anonymous");
    assert_eq!(body["user"]["dashboard_sort_mode"], "group");
}

// ---------------------------------------------------------------------------
// Auth — dashboard sort mode preference
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sort_mode_defaults_to_az_when_unset() {
    let app = TestApp::new().await;
    app.seed_admin("alice", "password123").await;
    let (_, cookie) = app.login("alice", "password123").await;

    let (status, body) = app.get_json("/api/v1/auth/me", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["dashboard_sort_mode"], "az");
}

#[tokio::test]
async fn test_update_sort_mode_persists_and_reflects_in_me() {
    let app = TestApp::new().await;
    app.seed_admin("alice", "password123").await;
    let (_, cookie) = app.login("alice", "password123").await;

    let (status, _) = app
        .put_json(
            "/api/v1/auth/me/sort-mode",
            serde_json::json!({"sort_mode": "group"}),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = app.get_json("/api/v1/auth/me", &cookie).await;
    assert_eq!(body["user"]["dashboard_sort_mode"], "group");
}

#[tokio::test]
async fn test_update_sort_mode_rejects_invalid_value() {
    let app = TestApp::new().await;
    app.seed_admin("alice", "password123").await;
    let (_, cookie) = app.login("alice", "password123").await;

    let (status, _) = app
        .put_json(
            "/api/v1/auth/me/sort-mode",
            serde_json::json!({"sort_mode": "bogus"}),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_update_sort_mode_unauthenticated_returns_401() {
    let app = TestApp::new().await;

    let (status, _) = app
        .put_json(
            "/api/v1/auth/me/sort-mode",
            serde_json::json!({"sort_mode": "group"}),
            "",
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
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

/// The `users` row — not the login-time session cache — decides the role.
///
/// Regression: the role was only ever written into the session at login, so a
/// session that outlived a role change (or predated roles entirely) pinned the
/// user to a stale role. A real admin was silently served as a viewer: the Add
/// button vanished and every write route 403'd.
#[tokio::test]
async fn test_role_is_read_from_db_not_stale_session() {
    let app = TestApp::new().await;
    app.seed_viewer("user", "password123").await;

    // Session is minted while the user is still a viewer.
    let (_, cookie) = app.login("user", "password123").await;
    let payload = serde_json::json!({"display_name": "Svc", "probe_enabled": false});
    let (status, _) = app
        .post_json("/api/v1/services", payload.clone(), &cookie)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Promote in the database, reusing the *same* session cookie.
    sqlx::query("UPDATE users SET role = 'admin' WHERE username = 'user'")
        .execute(&app.pool)
        .await
        .unwrap();

    let (status, _) = app.post_json("/api/v1/services", payload, &cookie).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "promotion in the DB must take effect on the existing session"
    );

    let (status, body) = app.get_json("/api/v1/auth/me", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["role"], "admin", "/me must report the DB role");
}

/// The mirror case: demotion in the database must revoke admin immediately
/// rather than waiting for the stale session to expire.
#[tokio::test]
async fn test_db_demotion_revokes_admin_on_existing_session() {
    let app = TestApp::new().await;
    app.seed_admin("boss", "password123").await;

    let (_, cookie) = app.login("boss", "password123").await;

    sqlx::query("UPDATE users SET role = 'viewer' WHERE username = 'boss'")
        .execute(&app.pool)
        .await
        .unwrap();

    let payload = serde_json::json!({"display_name": "Svc", "probe_enabled": false});
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

#[tokio::test]
async fn test_control_unknown_service_returns_404() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let (status, _) = app
        .post_json(
            "/api/v1/services/99999/start",
            serde_json::json!({}),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A manual, URL-only service has no systemd unit or container backing it —
/// control routes must reject it before ever reaching D-Bus/Docker.
#[tokio::test]
async fn test_control_manual_service_returns_400() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let payload = serde_json::json!({
        "display_name": "Manual Service",
        "url": "http://localhost:8080",
        "probe_enabled": false
    });
    let (_, created) = app.post_json("/api/v1/services", payload, &cookie).await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = app
        .post_json(
            &format!("/api/v1/services/{id}/stop"),
            serde_json::json!({}),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("isn't backed by a systemd unit or container"));
}

#[tokio::test]
async fn test_create_notification_channel_rejects_invalid_kind() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let payload = serde_json::json!({
        "name": "Bad Channel",
        "kind": "sms",
        "target": "https://example.com/hook"
    });
    let (status, body) = app
        .post_json("/api/v1/notifications/channels", payload, &cookie)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("kind"));
}

#[tokio::test]
async fn test_create_list_and_delete_notification_channel() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let payload = serde_json::json!({
        "name": "Ops Discord",
        "kind": "discord",
        "target": "https://discord.com/api/webhooks/123/abc",
        "events": ["service.down"]
    });
    let (status, created) = app
        .post_json("/api/v1/notifications/channels", payload, &cookie)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_i64().unwrap();

    let (list_status, list_body) = app
        .get_json("/api/v1/notifications/channels", &cookie)
        .await;
    assert_eq!(list_status, StatusCode::OK);
    let channels = list_body.as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["name"], "Ops Discord");
    // secret must never round-trip in a response, even when unset.
    assert!(channels[0].get("secret").is_none());

    let (del_status, _) = app
        .delete_req(&format!("/api/v1/notifications/channels/{id}"), &cookie)
        .await;
    assert_eq!(del_status, StatusCode::OK);
}

#[tokio::test]
async fn test_test_unknown_notification_channel_returns_404() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let (status, _) = app
        .post_json(
            "/api/v1/notifications/channels/99999/test",
            serde_json::json!({}),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_logs_stream_unknown_service_returns_404() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let (status, _) = app
        .get_json("/api/v1/services/99999/logs/stream", &cookie)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_logs_stream_manual_service_returns_400() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let payload = serde_json::json!({
        "display_name": "Manual Service",
        "url": "http://localhost:8080",
        "probe_enabled": false
    });
    let (_, created) = app.post_json("/api/v1/services", payload, &cookie).await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = app
        .get_json(&format!("/api/v1/services/{id}/logs/stream"), &cookie)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("isn't backed by a systemd unit or container"));
}

#[tokio::test]
async fn test_config_import_is_additive_and_dedupes_on_reimport() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let bundle = serde_json::json!({
        "version": 1,
        "exported_at": "2026-01-01T00:00:00Z",
        "groups": [{"name": "Media", "icon": null, "color": null, "sort_order": 0}],
        "services": [{
            "systemd_unit": "jellyfin.service",
            "discovery_source": null,
            "display_name": "Jellyfin",
            "description": null,
            "url": "http://localhost:8096",
            "icon": null,
            "group_name": "Media",
            "sort_order": 0,
            "probe_enabled": false,
            "probe_interval": 30,
            "tags": null,
            "visible": true,
            "skip_tls_verify": false
        }],
        "quick_links": [],
        "notification_channels": [],
        "settings": {"auth_mode": null}
    });

    // First import creates the group and the service.
    let (status1, summary1) = app
        .post_json("/api/v1/config/import", bundle.clone(), &cookie)
        .await;
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(summary1["groups_created"], 1);
    assert_eq!(summary1["services_created"], 1);

    // Re-importing the identical bundle must not duplicate anything: the
    // group is reused (unique by name) and the service is skipped (its
    // systemd_unit is already claimed) — additive-only, never destructive,
    // never a silent duplicate either.
    let (status2, summary2) = app
        .post_json("/api/v1/config/import", bundle, &cookie)
        .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(summary2["groups_created"], 0);
    assert_eq!(summary2["groups_reused"], 1);
    assert_eq!(summary2["services_created"], 0);
    assert_eq!(summary2["services_skipped"], 1);

    let (_, groups) = app.get_json("/api/v1/groups", &cookie).await;
    assert_eq!(
        groups.as_array().unwrap().len(),
        1,
        "group must not be duplicated"
    );
    let (_, services) = app.get_json("/api/v1/services", &cookie).await;
    assert_eq!(
        services.as_array().unwrap().len(),
        1,
        "service must not be duplicated"
    );
}

#[tokio::test]
async fn test_export_nix_excludes_secrets() {
    let app = TestApp::new().await;
    app.seed_admin("admin", "password123").await;
    let (_, cookie) = app.login("admin", "password123").await;

    let (status, body) = app.get_text("/api/v1/config/export/nix", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("services.vexboard.settings"));
    assert!(body.contains("discovery.enabled"));
    // The session-signing secret and webhook-signing secret are credentials —
    // never allowed into generated Nix, per this app's own secretFile convention.
    assert!(!body.contains("test-secret"));
    assert!(!body.contains("auth.secret"));
    assert!(!body.contains("webhook_secret"));
}
