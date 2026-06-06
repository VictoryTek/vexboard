use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i64,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub created_at: Option<NaiveDateTime>,
}

// --- Request/Response DTOs ---

#[derive(Debug, Deserialize)]
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
}

#[derive(Debug, Deserialize)]
pub struct UpdateService {
    pub discovery_source: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: Option<i64>,
    pub probe_enabled: Option<bool>,
    pub probe_interval: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub visible: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroup {
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroup {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceWithStatus {
    #[serde(flatten)]
    pub service: Service,
    pub status: String,
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuickLink {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateQuickLink {
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
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

#[derive(Debug, Deserialize)]
pub struct UpdateQuickLink {
    pub title: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i64>,
}
