use std::path::PathBuf;

use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use chrono::Utc;
use db::{
    database_stats::{
        AnalyzeResult, DatabaseStats, VacuumResult, analyze_database, get_database_stats,
        vacuum_database,
    },
    models::workspace::Workspace,
};
use deployment::Deployment;
use serde::{Deserialize, Serialize};
use services::services::container::ContainerService;
use sqlx::SqlitePool;
use ts_rs::TS;
use utils::{assets::asset_dir, execution_logs::EXECUTION_LOGS_DIRNAME, response::ApiResponse};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

const VACUUM_COOLDOWN_SECS: i64 = 5 * 60;
const DEFAULT_OLDER_THAN_DAYS: i64 = 14;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedStatsResponse {
    pub count: i64,
    pub older_than_days: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedNonTerminalResponse {
    pub workspace_ids: Vec<Uuid>,
    pub count: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedPurgeResult {
    pub deleted: i64,
    pub skipped_active: i64,
    pub older_than_days: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogStatsResponse {
    pub file_count: i64,
    pub total_bytes: i64,
    pub older_than_days: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogPurgeResult {
    pub deleted_files: i64,
    pub bytes_freed: i64,
    pub older_than_days: i64,
}

#[derive(Debug, Deserialize)]
pub struct OlderThanQuery {
    #[serde(default = "default_older_than_days")]
    pub older_than_days: i64,
}

fn default_older_than_days() -> i64 {
    DEFAULT_OLDER_THAN_DAYS
}

fn db_file_path() -> PathBuf {
    asset_dir().join("db.v2.sqlite")
}

async fn get_stats(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DatabaseStats>>, ApiError> {
    let pool = &deployment.db().pool;
    let db_path = db_file_path();
    let stats = get_database_stats(pool, &db_path).await.map_err(|e| {
        tracing::error!("database stats error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;
    Ok(ResponseJson(ApiResponse::success(stats)))
}

async fn vacuum(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<VacuumResult>>, ApiError> {
    {
        let last = deployment.last_vacuum_time().read().await;
        if let Some(prev) = *last {
            let elapsed = Utc::now().signed_duration_since(prev).num_seconds();
            if elapsed < VACUUM_COOLDOWN_SECS {
                return Err(ApiError::TooManyRequests(
                    "Vacuum cooldown active".to_string(),
                ));
            }
        }
    }

    let pool = &deployment.db().pool;
    let result = vacuum_database(pool).await.map_err(|e| {
        tracing::error!("vacuum error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;

    {
        let mut last = deployment.last_vacuum_time().write().await;
        *last = Some(Utc::now());
    }

    Ok(ResponseJson(ApiResponse::success(result)))
}

async fn analyze(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<AnalyzeResult>>, ApiError> {
    let pool = &deployment.db().pool;
    let result = analyze_database(pool).await.map_err(|e| {
        tracing::error!("analyze error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;
    Ok(ResponseJson(ApiResponse::success(result)))
}

async fn archived_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedStatsResponse>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }
    let pool = &deployment.db().pool;
    let cutoff = format!("-{} days", query.older_than_days);
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces
           WHERE archived = 1 AND updated_at < datetime('now', ?)"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;

    Ok(ResponseJson(ApiResponse::success(ArchivedStatsResponse {
        count,
        older_than_days: query.older_than_days,
    })))
}

async fn archived_non_terminal(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ArchivedNonTerminalResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let workspace_ids = fetch_archived_non_terminal_ids(pool).await?;
    let count = workspace_ids.len() as i64;
    Ok(ResponseJson(ApiResponse::success(
        ArchivedNonTerminalResponse {
            workspace_ids,
            count,
        },
    )))
}

async fn fetch_archived_non_terminal_ids(pool: &SqlitePool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        r#"SELECT w.id
           FROM workspaces w
           WHERE w.archived = 1
             AND EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = w.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )"#,
    )
    .fetch_all(pool)
    .await
}

async fn purge_archived(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedPurgeResult>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }
    let pool = &deployment.db().pool;
    let cutoff = format!("-{} days", query.older_than_days);

    // Count workspaces that match the age filter but are excluded due to active processes.
    let skipped_active: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces w
           WHERE w.archived = 1 AND w.updated_at < datetime('now', ?)
             AND EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = w.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;

    let candidates = sqlx::query_as::<_, Workspace>(
        r#"SELECT
                w.id,
                w.task_id,
                w.container_ref,
                w.branch,
                w.setup_completed_at,
                w.created_at,
                w.updated_at,
                w.archived,
                w.pinned,
                w.name,
                w.worktree_deleted
           FROM workspaces w
           WHERE w.archived = 1 AND w.updated_at < datetime('now', ?)
             AND NOT EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = w.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )"#,
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    let mut deleted = 0i64;
    for workspace in &candidates {
        if let Err(e) = deployment.container().delete(workspace).await {
            tracing::warn!(
                workspace_id = %workspace.id,
                "Failed to delete container for archived workspace: {}",
                e
            );
            continue;
        }

        match Workspace::delete(pool, workspace.id).await {
            Ok(_) => deleted += 1,
            Err(e) => tracing::warn!(
                workspace_id = %workspace.id,
                "Failed to delete workspace row after container delete: {}",
                e
            ),
        }
    }

    Ok(ResponseJson(ApiResponse::success(ArchivedPurgeResult {
        deleted,
        skipped_active,
        older_than_days: query.older_than_days,
    })))
}

async fn log_stats(
    State(_deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogStatsResponse>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    let (file_count, total_bytes) = tokio::task::spawn_blocking(move || {
        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(older_than_days as u64 * 86400);
        let mut count: i64 = 0;
        let mut bytes: i64 = 0;
        walk_log_files(&log_root, cutoff, &mut |meta| {
            count += 1;
            bytes += meta.len() as i64;
        });
        (count, bytes)
    })
    .await
    .map_err(|e| {
        tracing::error!("log_stats join error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;

    Ok(ResponseJson(ApiResponse::success(LogStatsResponse {
        file_count,
        total_bytes,
        older_than_days: query.older_than_days,
    })))
}

async fn purge_logs(
    State(_deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogPurgeResult>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    let (deleted_files, bytes_freed) = tokio::task::spawn_blocking(move || {
        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(older_than_days as u64 * 86400);

        let entries = collect_old_log_files(&log_root, cutoff);
        let mut deleted: i64 = 0;
        let mut freed: i64 = 0;

        for (path, size) in entries {
            if std::fs::remove_file(&path).is_ok() {
                deleted += 1;
                freed += size as i64;
            } else {
                tracing::warn!("Failed to delete log file: {}", path.display());
            }
        }

        (deleted, freed)
    })
    .await
    .map_err(|e| {
        tracing::error!("purge_logs join error: {e}");
        ApiError::Database(sqlx::Error::Protocol(e.to_string()))
    })?;

    Ok(ResponseJson(ApiResponse::success(LogPurgeResult {
        deleted_files,
        bytes_freed,
        older_than_days: query.older_than_days,
    })))
}

/// Walk `.jsonl` log files older than `cutoff`, calling `cb` with each file's metadata.
fn walk_log_files(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
    cb: &mut impl FnMut(&std::fs::Metadata),
) {
    let Ok(top) = std::fs::read_dir(root) else {
        return;
    };
    for prefix_entry in top.flatten() {
        let Ok(sessions_dir) = std::fs::read_dir(prefix_entry.path()) else {
            continue;
        };
        for session_entry in sessions_dir.flatten() {
            let processes_dir = session_entry.path().join("processes");
            let Ok(procs) = std::fs::read_dir(&processes_dir) else {
                continue;
            };
            for proc_entry in procs.flatten() {
                let path = proc_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime < cutoff {
                            cb(&meta);
                        }
                    }
                }
            }
        }
    }
}

/// Collect `(path, size)` for `.jsonl` log files older than `cutoff`.
fn collect_old_log_files(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
) -> Vec<(std::path::PathBuf, u64)> {
    let mut result = Vec::new();
    let Ok(top) = std::fs::read_dir(root) else {
        return result;
    };
    for prefix_entry in top.flatten() {
        let Ok(sessions_dir) = std::fs::read_dir(prefix_entry.path()) else {
            continue;
        };
        for session_entry in sessions_dir.flatten() {
            let processes_dir = session_entry.path().join("processes");
            let Ok(procs) = std::fs::read_dir(&processes_dir) else {
                continue;
            };
            for proc_entry in procs.flatten() {
                let path = proc_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime < cutoff {
                            result.push((path, meta.len()));
                        }
                    }
                }
            }
        }
    }
    result
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/database/stats", get(get_stats))
        .route("/database/vacuum", post(vacuum))
        .route("/database/analyze", post(analyze))
        .route("/database/archived-stats", get(archived_stats))
        .route("/database/archived-non-terminal", get(archived_non_terminal))
        .route("/database/purge-archived", post(purge_archived))
        .route("/database/log-stats", get(log_stats))
        .route("/database/purge-logs", post(purge_logs))
}
