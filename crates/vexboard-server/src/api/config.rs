use axum::{extract::State, Json};
use serde_json::json;

use crate::AppState;

pub(crate) async fn public_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "icon_cdn_base": state.config.server.icon_cdn_base
    }))
}
