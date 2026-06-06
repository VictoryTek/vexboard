use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "cookieAuth",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("session_id"))),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "VexBoard API",
        version = "0.1.0",
        description = "Self-hosted server dashboard REST API",
        license(name = "MIT"),
        contact(name = "VexBoard", email = "victorytech@proton.me"),
    ),
    paths(
        crate::api::health::health_check,
        crate::api::setup::status,
        crate::api::setup::create_admin,
        crate::api::auth::login,
        crate::api::auth::logout,
        crate::api::auth::me,
        crate::api::auth::update_me,
        crate::api::services::list_services,
        crate::api::services::create_service,
        crate::api::services::update_service,
        crate::api::services::delete_service,
        crate::api::services::claim_service,
        crate::api::services::reorder_services,
        crate::api::groups::list_groups,
        crate::api::groups::create_group,
        crate::api::groups::update_group,
        crate::api::groups::delete_group,
        crate::api::quick_links::list_quick_links,
        crate::api::quick_links::create_quick_link,
        crate::api::quick_links::update_quick_link,
        crate::api::quick_links::delete_quick_link,
        crate::api::audit::list_audit,
        crate::api::metrics::metrics_snapshot,
        crate::api::metrics::metrics_stream,
        crate::discovery::list_discovered,
        crate::discovery::trigger_refresh,
    ),
    components(
        schemas(
            crate::db::models::Group,
            crate::db::models::Service,
            crate::db::models::CreateService,
            crate::db::models::UpdateService,
            crate::db::models::CreateGroup,
            crate::db::models::UpdateGroup,
            crate::db::models::LoginRequest,
            crate::db::models::UserInfo,
            crate::db::models::ServiceWithStatus,
            crate::db::models::QuickLink,
            crate::db::models::CreateQuickLink,
            crate::db::models::UpdateQuickLink,
            crate::db::models::ReorderItem,
            crate::db::models::AuditEvent,
            crate::api::setup::SetupRequest,
            crate::discovery::DiscoveredUnit,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Health check"),
        (name = "setup", description = "Initial admin setup (one-time bootstrap)"),
        (name = "auth", description = "Authentication (login, logout, profile)"),
        (name = "services", description = "Service management"),
        (name = "groups", description = "Group management"),
        (name = "quick-links", description = "Quick link management"),
        (name = "audit", description = "Audit log — requires authentication"),
        (name = "metrics", description = "System metrics (REST snapshot + SSE stream)"),
        (name = "discovery", description = "Auto-discovery of systemd units and containers"),
    )
)]
pub struct ApiDoc;
