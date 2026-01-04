use axum::{extract::State, response::Json};
use deployment::Deployment;
use serde::Serialize;
use utils::build_info::BUILD_INFO;

use crate::DeploymentImpl;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub git_commit: &'static str,
    pub git_branch: &'static str,
    pub build_timestamp: &'static str,
    pub database_ready: bool,
}

pub async fn health_check(State(deployment): State<DeploymentImpl>) -> Json<HealthResponse> {
    // Quick database connectivity check
    let database_ready = sqlx::query("SELECT 1")
        .fetch_one(&deployment.db().pool)
        .await
        .is_ok();

    Json(HealthResponse {
        status: if database_ready { "ok" } else { "degraded" },
        version: BUILD_INFO.version,
        git_commit: BUILD_INFO.git_commit,
        git_branch: BUILD_INFO.git_branch,
        build_timestamp: BUILD_INFO.build_timestamp,
        database_ready,
    })
}
