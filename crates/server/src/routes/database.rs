//! Database statistics and maintenance API routes.
//!
//! Provides endpoints for:
//! - GET /api/database/stats - Retrieve database statistics
//! - POST /api/database/vacuum - Run VACUUM to reclaim space
//! - POST /api/database/analyze - Run ANALYZE for query optimisation
//! - GET /api/database/archived-stats - Count archived terminal tasks for purging
//! - GET /api/database/archived-non-terminal - List stuck archived tasks
//! - POST /api/database/purge-archived - Delete archived terminal tasks
//! - GET /api/database/log-stats - Count log entries for purging
//! - POST /api/database/purge-logs - Delete old log entries

use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::{
    DatabaseStats, VacuumResult, analyze_database, get_database_stats,
    models::log_entry::DbLogEntry, models::task::Task, vacuum_database,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::{assets::database_path, response::ApiResponse};

use crate::{DeploymentImpl, error::ApiError};

/// Query parameters for archived task endpoints.
#[derive(Debug, Deserialize)]
pub struct ArchivedStatsQuery {
    /// Number of days old a task must be to be included. Defaults to 14.
    #[serde(default = "default_older_than_days")]
    pub older_than_days: i64,
}

fn default_older_than_days() -> i64 {
    14
}

/// Response for archived task stats query.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ArchivedStatsResponse {
    /// Number of archived terminal tasks older than the cutoff.
    pub count: i64,
    /// The cutoff in days used for the query.
    pub older_than_days: i64,
}

/// Response for archived task purge operation.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ArchivedPurgeResult {
    /// Number of tasks deleted.
    pub deleted: i64,
    /// The cutoff in days used for the purge.
    pub older_than_days: i64,
}

/// Query parameters for log entry endpoints.
#[derive(Debug, Deserialize)]
pub struct LogStatsQuery {
    /// Number of days old a log entry must be to be included. Defaults to 14.
    #[serde(default = "default_older_than_days")]
    pub older_than_days: i64,
}

/// Response for log entry stats query.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct LogStatsResponse {
    /// Number of log entries older than the cutoff.
    pub count: i64,
    /// The cutoff in days used for the query.
    pub older_than_days: i64,
}

/// Response for log entry purge operation.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct LogPurgeResult {
    /// Number of log entries deleted.
    pub deleted: i64,
    /// The cutoff in days used for the purge.
    pub older_than_days: i64,
}

/// GET /api/database/stats
///
/// Returns database statistics including file sizes, page info, and table counts.
async fn get_stats(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DatabaseStats>>, ApiError> {
    let db_path = database_path();
    let pool = &deployment.db().pool;

    let stats = get_database_stats(pool, &db_path)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(stats)))
}

/// POST /api/database/vacuum
///
/// Runs VACUUM on the database to reclaim space from deleted records.
/// Returns the bytes freed by the operation.
///
/// Rate limited to once per 5 minutes to prevent accidental repeated calls
/// that could lock the database.
async fn vacuum(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<VacuumResult>>, ApiError> {
    const VACUUM_COOLDOWN_SECS: u64 = 300; // 5 minutes

    // Check cooldown
    {
        let last_time = deployment.last_vacuum_time().read().await;
        if let Some(time) = *last_time
            && time.elapsed() < std::time::Duration::from_secs(VACUUM_COOLDOWN_SECS)
        {
            let remaining = VACUUM_COOLDOWN_SECS - time.elapsed().as_secs();
            return Err(ApiError::TooManyRequests(format!(
                "Please wait {} seconds before running VACUUM again",
                remaining
            )));
        }
    }

    let db_path = database_path();
    let pool = &deployment.db().pool;

    let result = vacuum_database(pool, &db_path)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // Update cooldown timestamp
    {
        let mut last_time = deployment.last_vacuum_time().write().await;
        *last_time = Some(std::time::Instant::now());
    }

    Ok(ResponseJson(ApiResponse::success(result)))
}

/// POST /api/database/analyze
///
/// Runs ANALYZE on the database to update query planner statistics.
async fn analyze(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    analyze_database(pool)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(())))
}

/// GET /api/database/archived-stats
///
/// Returns the count of archived tasks in terminal states (done/cancelled)
/// that are older than the specified number of days.
async fn archived_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ArchivedStatsQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedStatsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    let count = Task::count_archived_terminal_older_than(pool, query.older_than_days)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(ArchivedStatsResponse {
        count,
        older_than_days: query.older_than_days,
    })))
}

/// GET /api/database/archived-non-terminal
///
/// Returns a list of archived tasks that are NOT in terminal states (done/cancelled).
/// These are "stuck" tasks that were archived but not completed.
async fn archived_non_terminal(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Task>>>, ApiError> {
    let pool = &deployment.db().pool;

    let tasks = Task::find_archived_non_terminal(pool)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(tasks)))
}

/// POST /api/database/purge-archived
///
/// Deletes archived tasks in terminal states (done/cancelled) that are older
/// than the specified number of days. Returns the number of tasks deleted.
async fn purge_archived(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ArchivedStatsQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedPurgeResult>>, ApiError> {
    let pool = &deployment.db().pool;

    let deleted = Task::delete_archived_terminal_older_than(pool, query.older_than_days)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(ArchivedPurgeResult {
        deleted,
        older_than_days: query.older_than_days,
    })))
}

/// GET /api/database/log-stats
///
/// Returns the count of log entries older than the specified number of days.
async fn log_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<LogStatsQuery>,
) -> Result<ResponseJson<ApiResponse<LogStatsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    let count = DbLogEntry::count_older_than(pool, query.older_than_days)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(LogStatsResponse {
        count,
        older_than_days: query.older_than_days,
    })))
}

/// POST /api/database/purge-logs
///
/// Deletes log entries older than the specified number of days.
/// Returns the number of log entries deleted.
async fn purge_logs(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<LogStatsQuery>,
) -> Result<ResponseJson<ApiResponse<LogPurgeResult>>, ApiError> {
    let pool = &deployment.db().pool;

    let deleted = DbLogEntry::delete_older_than(pool, query.older_than_days)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(LogPurgeResult {
        deleted,
        older_than_days: query.older_than_days,
    })))
}

/// Create the database routes router.
pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/database/stats", get(get_stats))
        .route("/database/vacuum", post(vacuum))
        .route("/database/analyze", post(analyze))
        .route("/database/archived-stats", get(archived_stats))
        .route(
            "/database/archived-non-terminal",
            get(archived_non_terminal),
        )
        .route("/database/purge-archived", post(purge_archived))
        .route("/database/log-stats", get(log_stats))
        .route("/database/purge-logs", post(purge_logs))
}
