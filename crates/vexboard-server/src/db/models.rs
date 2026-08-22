use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Service {
    pub id: i64,
    pub systemd_unit: Option<String>,
    pub discovery_source: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: i64,
    pub probe_enabled: bool,
    pub probe_interval: i64,
    pub tags: Option<String>,
    pub visible: bool,
    pub skip_tls_verify: bool,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

#[cfg(not(all(unix, feature = "pam-auth")))]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[schema(value_type = String, write_only = true)]
    pub password_hash: String,
    pub role: String,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct UserPublic {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: Option<NaiveDateTime>,
}

// --- Request/Response DTOs ---

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateService {
    pub systemd_unit: Option<String>,
    pub discovery_source: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: Option<i64>,
    pub probe_enabled: Option<bool>,
    pub probe_interval: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub visible: Option<bool>,
    pub skip_tls_verify: Option<bool>,
}

/// Distinguishes "field omitted" (`None`) from "field explicitly `null`" (`Some(None)`) in a
/// partial-update JSON body, so a PUT payload can request clearing a nullable column instead of
/// that key's absence being silently treated as "keep existing value."
fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateService {
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>)]
    pub discovery_source: Option<Option<String>>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<i64>)]
    pub group_id: Option<Option<i64>>,
    pub sort_order: Option<i64>,
    pub probe_enabled: Option<bool>,
    pub probe_interval: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub visible: Option<bool>,
    pub skip_tls_verify: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateGroup {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateGroup {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>)]
    pub icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>)]
    pub color: Option<Option<String>>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServiceWithStatus {
    #[serde(flatten)]
    pub service: Service,
    pub status: String,
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ProbeHistoryPoint {
    pub status: String,
    pub latency_ms: Option<i64>,
    pub checked_at: Option<NaiveDateTime>,
}

/// A single maximal run of consecutive non-"up" probe results.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Incident {
    /// The last non-"up" status seen during the run (`"down"` or `"unknown"`).
    pub status: String,
    pub started_at: NaiveDateTime,
    /// `None` while the incident is still ongoing (no recovery check yet).
    pub ended_at: Option<NaiveDateTime>,
    /// Seconds from `started_at` to `ended_at`, or to now while ongoing.
    pub duration_secs: i64,
    pub check_count: i64,
}

/// Uptime percentages, recent heartbeats, and derived incidents for one service.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UptimeSummary {
    /// `None` when there are no probe results within the window yet.
    pub uptime_24h: Option<f64>,
    pub uptime_7d: Option<f64>,
    pub uptime_30d: Option<f64>,
    /// The most recent probe results, oldest-first.
    pub heartbeats: Vec<ProbeHistoryPoint>,
    /// Most-recent-first.
    pub incidents: Vec<Incident>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct QuickLink {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateQuickLink {
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct AuditEvent {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<i64>,
    pub detail: Option<String>,
    pub ip_addr: Option<String>,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateQuickLink {
    pub title: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<i64>)]
    pub group_id: Option<Option<i64>>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReorderItem {
    pub id: i64,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct NotificationChannel {
    pub id: i64,
    pub name: String,
    /// `"webhook"`, `"discord"`, or `"ntfy"`.
    pub kind: String,
    pub target: String,
    /// Write-only, like a password hash — never sent back to the client.
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    /// JSON array of event types as text (e.g. `["service.down"]`); empty
    /// array means all events. Exposed as-is, matching how `Service.tags`
    /// already round-trips a JSON-array-as-text column to the frontend.
    pub events: String,
    pub enabled: bool,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateNotificationChannel {
    pub name: String,
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateNotificationChannel {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub target: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>)]
    pub secret: Option<Option<String>>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

// --- Portable config export/import ---
//
// Deliberately separate from the read-models above: these are id-free (a
// group's numeric id isn't portable across instances — a service/quick-link
// references its group by name instead) and omit anything that shouldn't
// round-trip through a shareable file (notification channel secrets, user
// passwords — users aren't exported at all).

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ExportedGroup {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportedService {
    pub systemd_unit: Option<String>,
    pub discovery_source: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub group_name: Option<String>,
    pub sort_order: i64,
    pub probe_enabled: bool,
    pub probe_interval: i64,
    pub tags: Option<Vec<String>>,
    pub visible: bool,
    pub skip_tls_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportedQuickLink {
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub group_name: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportedNotificationChannel {
    pub name: String,
    pub kind: String,
    pub target: String,
    pub events: Vec<String>,
    pub enabled: bool,
}

/// Exported for reference only — never re-applied on import. Silently
/// changing whether the whole dashboard requires login, driven by an
/// uploaded file, is too security-sensitive to automate; the admin
/// changes it explicitly via the Security tab.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportedSettings {
    pub auth_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigExport {
    pub version: u32,
    /// RFC3339 timestamp.
    pub exported_at: String,
    pub groups: Vec<ExportedGroup>,
    pub services: Vec<ExportedService>,
    pub quick_links: Vec<ExportedQuickLink>,
    pub notification_channels: Vec<ExportedNotificationChannel>,
    pub settings: ExportedSettings,
}

/// Per-category created/skipped counts returned by import — the tractable
/// version of "you'll see what changed," short of a full pre-commit diff.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ConfigImportSummary {
    pub groups_created: i64,
    pub groups_reused: i64,
    pub services_created: i64,
    pub services_skipped: i64,
    pub quick_links_created: i64,
    pub notification_channels_created: i64,
    pub notification_channels_skipped: i64,
}
