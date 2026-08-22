use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde_json::json;
use tower_sessions::Session;

use crate::db;
use crate::db::models::{
    ConfigExport, ConfigImportSummary, ExportedGroup, ExportedNotificationChannel,
    ExportedQuickLink, ExportedService, ExportedSettings,
};
use crate::AppState;

const EXPORT_VERSION: u32 = 1;
const VALID_CHANNEL_KINDS: [&str; 3] = ["webhook", "discord", "ntfy"];

/// Every route here is admin-only. `export`/`export_nix` are read-only;
/// `import` is additive-only — it never deletes or overwrites existing rows.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/export", get(export_config))
        .route("/export/nix", get(export_nix))
        .route("/import", axum::routing::post(import_config))
}

#[derive(sqlx::FromRow)]
struct ServiceExportRow {
    systemd_unit: Option<String>,
    discovery_source: Option<String>,
    display_name: String,
    description: Option<String>,
    url: Option<String>,
    icon: Option<String>,
    group_name: Option<String>,
    sort_order: i64,
    probe_enabled: bool,
    probe_interval: i64,
    tags: Option<String>,
    visible: bool,
    skip_tls_verify: bool,
}

#[derive(sqlx::FromRow)]
struct QuickLinkExportRow {
    title: String,
    url: String,
    icon: Option<String>,
    description: Option<String>,
    group_name: Option<String>,
    sort_order: i64,
}

#[derive(sqlx::FromRow)]
struct ChannelExportRow {
    name: String,
    kind: String,
    target: String,
    events: String,
    enabled: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/config/export",
    tag = "config",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Portable JSON backup of groups, services, quick links, and notification channels", body = ConfigExport),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn export_config(State(state): State<AppState>) -> axum::response::Response {
    let export = match build_export(&state).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to build config export: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to build export"})),
            )
                .into_response();
        }
    };

    let mut response = Json(export).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_static("attachment; filename=\"vexboard-config.json\""),
    );
    response
}

async fn build_export(state: &AppState) -> anyhow::Result<ConfigExport> {
    let groups = sqlx::query_as::<_, ExportedGroup>(
        "SELECT name, icon, color, sort_order FROM groups ORDER BY sort_order ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let service_rows = sqlx::query_as::<_, ServiceExportRow>(
        "SELECT s.systemd_unit, s.discovery_source, s.display_name, s.description, s.url, s.icon, \
         g.name AS group_name, s.sort_order, s.probe_enabled, s.probe_interval, s.tags, s.visible, s.skip_tls_verify \
         FROM services s LEFT JOIN groups g ON s.group_id = g.id ORDER BY s.sort_order ASC",
    )
    .fetch_all(&state.db)
    .await?;
    let services = service_rows
        .into_iter()
        .map(|r| ExportedService {
            systemd_unit: r.systemd_unit,
            discovery_source: r.discovery_source,
            display_name: r.display_name,
            description: r.description,
            url: r.url,
            icon: r.icon,
            group_name: r.group_name,
            sort_order: r.sort_order,
            probe_enabled: r.probe_enabled,
            probe_interval: r.probe_interval,
            tags: r
                .tags
                .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()),
            visible: r.visible,
            skip_tls_verify: r.skip_tls_verify,
        })
        .collect();

    let link_rows = sqlx::query_as::<_, QuickLinkExportRow>(
        "SELECT ql.title, ql.url, ql.icon, ql.description, g.name AS group_name, ql.sort_order \
         FROM quick_links ql LEFT JOIN groups g ON ql.group_id = g.id ORDER BY ql.sort_order ASC",
    )
    .fetch_all(&state.db)
    .await?;
    let quick_links = link_rows
        .into_iter()
        .map(|r| ExportedQuickLink {
            title: r.title,
            url: r.url,
            icon: r.icon,
            description: r.description,
            group_name: r.group_name,
            sort_order: r.sort_order,
        })
        .collect();

    let channel_rows = sqlx::query_as::<_, ChannelExportRow>(
        "SELECT name, kind, target, events, enabled FROM notification_channels ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await?;
    let notification_channels = channel_rows
        .into_iter()
        .map(|r| ExportedNotificationChannel {
            name: r.name,
            kind: r.kind,
            target: r.target,
            events: serde_json::from_str(&r.events).unwrap_or_default(),
            enabled: r.enabled,
        })
        .collect();

    let auth_mode = db::get_setting(&state.db, "auth_mode").await.ok().flatten();

    Ok(ConfigExport {
        version: EXPORT_VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        groups,
        services,
        quick_links,
        notification_channels,
        settings: ExportedSettings { auth_mode },
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/config/import",
    tag = "config",
    security(("cookieAuth" = [])),
    request_body = ConfigExport,
    responses(
        (status = 200, description = "Import complete; returns created/skipped counts", body = ConfigImportSummary),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
        (status = 500, description = "Database error"),
    )
)]
#[tracing::instrument(skip(state, session, payload))]
pub(crate) async fn import_config(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<ConfigExport>,
) -> impl IntoResponse {
    let mut summary = ConfigImportSummary {
        groups_created: 0,
        groups_reused: 0,
        services_created: 0,
        services_skipped: 0,
        quick_links_created: 0,
        notification_channels_created: 0,
        notification_channels_skipped: 0,
    };

    for g in &payload.groups {
        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM groups WHERE name = ?")
            .bind(&g.name)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
        if existing.is_some() {
            summary.groups_reused += 1;
            continue;
        }
        if sqlx::query("INSERT INTO groups (name, icon, color, sort_order) VALUES (?, ?, ?, ?)")
            .bind(&g.name)
            .bind(&g.icon)
            .bind(&g.color)
            .bind(g.sort_order)
            .execute(&state.db)
            .await
            .is_ok()
        {
            summary.groups_created += 1;
        }
    }

    // Re-read the complete current table (not just this bundle's groups) so a
    // service/quick-link referencing a group that already existed outside the
    // import still resolves correctly.
    let group_ids: std::collections::HashMap<String, i64> =
        sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM groups")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, name)| (name, id))
            .collect();

    for s in &payload.services {
        if let Some(unit) = &s.systemd_unit {
            let taken: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM services WHERE systemd_unit = ? LIMIT 1)",
            )
            .bind(unit)
            .fetch_one(&state.db)
            .await
            .unwrap_or(true);
            if taken {
                summary.services_skipped += 1;
                continue;
            }
        }

        let group_id = s
            .group_name
            .as_ref()
            .and_then(|n| group_ids.get(n).copied());
        let tags_json = s.tags.as_ref().and_then(|t| serde_json::to_string(t).ok());

        let result = sqlx::query(
            "INSERT INTO services (systemd_unit, discovery_source, display_name, description, url, icon, \
             group_id, sort_order, probe_enabled, probe_interval, tags, visible, skip_tls_verify) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.systemd_unit)
        .bind(&s.discovery_source)
        .bind(&s.display_name)
        .bind(&s.description)
        .bind(&s.url)
        .bind(&s.icon)
        .bind(group_id)
        .bind(s.sort_order)
        .bind(s.probe_enabled)
        .bind(s.probe_interval)
        .bind(&tags_json)
        .bind(s.visible)
        .bind(s.skip_tls_verify)
        .execute(&state.db)
        .await;

        if result.is_ok() {
            summary.services_created += 1;
        } else {
            summary.services_skipped += 1;
        }
    }

    for l in &payload.quick_links {
        let group_id = l
            .group_name
            .as_ref()
            .and_then(|n| group_ids.get(n).copied());
        if sqlx::query(
            "INSERT INTO quick_links (title, url, icon, description, group_id, sort_order) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&l.title)
        .bind(&l.url)
        .bind(&l.icon)
        .bind(&l.description)
        .bind(group_id)
        .bind(l.sort_order)
        .execute(&state.db)
        .await
        .is_ok()
        {
            summary.quick_links_created += 1;
        }
    }

    for c in &payload.notification_channels {
        if !VALID_CHANNEL_KINDS.contains(&c.kind.as_str()) {
            summary.notification_channels_skipped += 1;
            continue;
        }
        let events_json = serde_json::to_string(&c.events).unwrap_or_else(|_| "[]".to_string());
        if sqlx::query(
            "INSERT INTO notification_channels (name, kind, target, events, enabled) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&c.name)
        .bind(&c.kind)
        .bind(&c.target)
        .bind(&events_json)
        .bind(c.enabled)
        .execute(&state.db)
        .await
        .is_ok()
        {
            summary.notification_channels_created += 1;
        } else {
            summary.notification_channels_skipped += 1;
        }
    }

    let actor = session
        .get::<String>("username")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    db::audit::insert(
        &state.db,
        &actor,
        "config.import",
        Some("config"),
        None,
        Some(&json!(summary).to_string()),
        None,
    )
    .await;

    (StatusCode::OK, Json(summary))
}

#[utoipa::path(
    get,
    path = "/api/v1/config/export/nix",
    tag = "config",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Current global settings as a services.vexboard.settings Nix attrset", content_type = "text/plain"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin role required"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn export_nix(State(state): State<AppState>) -> impl IntoResponse {
    let c = &state.config;

    fn nix_list(items: &[String]) -> String {
        if items.is_empty() {
            return "[ ]".to_string();
        }
        let quoted: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
        format!("[ {} ]", quoted.join(" "))
    }

    // auth.secret and notifications.webhook_secret are deliberately never
    // included — those are credentials, and this app's own Nix module
    // convention keeps secrets out of Nix source (a separate `secretFile`).
    let nix = format!(
        "services.vexboard.settings = {{\n\
         \x20\x20auth.secure_cookies = {};\n\
         \x20\x20auth.login_rate_limit_attempts = {};\n\
         \x20\x20auth.login_rate_limit_window_secs = {};\n\
         \x20\x20auth.behind_proxy = {};\n\
         \x20\x20discovery.enabled = {};\n\
         \x20\x20discovery.interval_secs = {};\n\
         \x20\x20discovery.server_services_only = {};\n\
         \x20\x20discovery.exclude_units = {};\n\
         \x20\x20docker.enabled = {};\n\
         \x20\x20docker.interval_secs = {};\n\
         \x20\x20docker.sockets = {};\n\
         \x20\x20docker.exclude_images = {};\n\
         \x20\x20probe.default_interval_secs = {};\n\
         \x20\x20probe.timeout_secs = {};\n\
         \x20\x20probe.history_retention_days = {};\n\
         \x20\x20metrics.push_interval_ms = {};\n\
         \x20\x20notifications.retry_count = {};\n\
         \x20\x20notifications.retry_delay_secs = {};\n\
         }};\n",
        c.auth.secure_cookies,
        c.auth.login_rate_limit_attempts,
        c.auth.login_rate_limit_window_secs,
        c.auth.behind_proxy,
        c.discovery.enabled,
        c.discovery.interval_secs,
        c.discovery.server_services_only,
        nix_list(&c.discovery.exclude_units),
        c.docker.enabled,
        c.docker.interval_secs,
        nix_list(&c.docker.sockets),
        nix_list(&c.docker.exclude_images),
        c.probe.default_interval_secs,
        c.probe.timeout_secs,
        c.probe.history_retention_days,
        c.metrics.push_interval_ms,
        c.notifications.retry_count,
        c.notifications.retry_delay_secs,
    );

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        nix,
    )
}
