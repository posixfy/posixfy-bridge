use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;

use crate::auth::middleware::AuthApiKeyOnly;
use crate::error::AppError;
use crate::AppState;

async fn list_mounts(
    State(state): State<AppState>,
    _auth: AuthApiKeyOnly,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!(state.mount_points)))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/mounts", get(list_mounts))
}
