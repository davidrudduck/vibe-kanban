use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use local_deployment::claude_hooks::ClaudeHookPayload;
use serde_json::json;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

async fn receive_hook(
    State(deployment): State<DeploymentImpl>,
    Path((execution_id, event)): Path<(Uuid, String)>,
    Json(payload): Json<ClaudeHookPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    deployment
        .handle_claude_hook(execution_id, event, payload)
        .await
        .map_err(|err| ApiError::BadRequest(err.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) fn router() -> Router<DeploymentImpl> {
    Router::new().route("/claude-hooks/{execution_id}/{event}", post(receive_hook))
}
