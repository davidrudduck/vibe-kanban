use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, post},
};
use relay_types::{ListHostReposResponse, ListRelayHostsResponse, ReportHostReposRequest};
use uuid::Uuid;

use super::error::ErrorResponse;
use crate::{AppState, auth::RequestContext, db::hosts::HostRepository};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/hosts", get(list_hosts))
        .route("/hosts/repos", post(report_host_repos))
        .route("/hosts/{host_id}/repos", get(list_host_repos))
}

async fn list_hosts(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
) -> Result<Json<ListRelayHostsResponse>, ErrorResponse> {
    let repo = HostRepository::new(state.pool());
    let hosts = repo
        .list_accessible_hosts(ctx.user.id)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to list relay hosts");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list hosts")
        })?;

    Ok(Json(ListRelayHostsResponse { hosts }))
}

async fn report_host_repos(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<ReportHostReposRequest>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let repo = HostRepository::new(state.pool());
    let host_id = repo
        .get_host_id_by_machine_id(ctx.user.id, &payload.machine_id)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to look up host by machine_id");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        })?
        .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "host not found"))?;

    repo.upsert_host_repos(host_id, &payload.repos)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to upsert host repos");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        })?;

    Ok(Json(serde_json::json!({})))
}

async fn list_host_repos(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(host_id): Path<Uuid>,
) -> Result<Json<ListHostReposResponse>, ErrorResponse> {
    let repo = HostRepository::new(state.pool());
    // Verify the user can access this host
    let hosts = repo
        .list_accessible_hosts(ctx.user.id)
        .await
        .map_err(|error| {
            tracing::warn!(?error, "failed to check host access");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "db error")
        })?;
    if !hosts.iter().any(|h| h.id == host_id) {
        return Err(ErrorResponse::new(StatusCode::FORBIDDEN, "access denied"));
    }

    let repos = repo.list_host_repos(host_id).await.map_err(|error| {
        tracing::warn!(?error, "failed to list host repos");
        ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })?;

    Ok(Json(ListHostReposResponse { repos }))
}
