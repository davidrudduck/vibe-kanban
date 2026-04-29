use std::path::{Path, PathBuf};

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
use utils::{
    assets::asset_dir, execution_logs::process_log_file_path_in_root, response::ApiResponse,
};
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

const VACUUM_COOLDOWN_SECS: i64 = 5 * 60;
const DEFAULT_OLDER_THAN_DAYS: i64 = 14;
const PURGE_LOG_BATCH_SIZE: usize = 100;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedStatsResponse {
    pub count: i64,
    pub older_than_days: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedPurgeResult {
    pub deleted: usize,
    pub skipped_active: usize,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogStatsResponse {
    pub file_count: u64,
    pub total_bytes: u64,
    pub older_than_days: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogPurgeResult {
    pub deleted_files: u64,
    pub bytes_freed: u64,
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
    let stats = get_database_stats(pool, &db_path)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to get database stats: {}", e)))?;
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
    let result = vacuum_database(pool)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Vacuum failed: {}", e)))?;

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
    let result = analyze_database(pool)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Analyze failed: {}", e)))?;
    Ok(ResponseJson(ApiResponse::success(result)))
}

async fn archived_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedStatsResponse>>, ApiError> {
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
) -> Result<ResponseJson<ApiResponse<Vec<Workspace>>>, ApiError> {
    let pool = &deployment.db().pool;
    let workspaces = fetch_archived_with_active_processes(pool).await?;
    Ok(ResponseJson(ApiResponse::success(workspaces)))
}

async fn fetch_archived_with_active_processes(
    pool: &SqlitePool,
) -> Result<Vec<Workspace>, sqlx::Error> {
    sqlx::query_as::<_, Workspace>(
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

    let mut deleted = 0usize;
    for workspace in &candidates {
        if let Err(e) = deployment.container().delete(workspace).await {
            tracing::warn!(
                "Failed to delete container for archived workspace {}: {}",
                workspace.id,
                e
            );
            continue;
        }

        match Workspace::delete(pool, workspace.id).await {
            Ok(_) => deleted += 1,
            Err(e) => tracing::warn!(
                "Failed to delete workspace row {} after container delete: {}",
                workspace.id,
                e
            ),
        }
    }

    Ok(ResponseJson(ApiResponse::success(ArchivedPurgeResult {
        deleted,
        skipped_active: skipped_active as usize,
    })))
}

async fn log_stats(
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogStatsResponse>>, ApiError> {
    let root = asset_dir().join(utils::execution_logs::EXECUTION_LOGS_DIRNAME);
    let cutoff_secs = (query.older_than_days.max(0) as u64) * 24 * 60 * 60;
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(cutoff_secs))
        .unwrap_or(std::time::UNIX_EPOCH);

    let (file_count, total_bytes) = tokio::task::spawn_blocking(move || {
        let mut count: u64 = 0;
        let mut bytes: u64 = 0;
        if root.exists() {
            walk_jsonl_files(&root, &mut |path, metadata| {
                if let Ok(modified) = metadata.modified()
                    && modified < cutoff
                {
                    count += 1;
                    bytes += metadata.len();
                }
                let _ = path;
            });
        }
        (count, bytes)
    })
    .await
    .map_err(|e| ApiError::BadRequest(format!("log_stats join error: {}", e)))?;

    Ok(ResponseJson(ApiResponse::success(LogStatsResponse {
        file_count,
        total_bytes,
        older_than_days: query.older_than_days,
    })))
}

fn walk_jsonl_files<F>(dir: &Path, visit: &mut F)
where
    F: FnMut(&Path, &std::fs::Metadata),
{
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            walk_jsonl_files(&path, visit);
        } else if metadata.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            visit(&path, &metadata);
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProcessLogRow {
    id: Uuid,
    session_id: Uuid,
}

async fn purge_logs(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogPurgeResult>>, ApiError> {
    let pool = &deployment.db().pool;
    let cutoff = format!("-{} days", query.older_than_days);

    let rows = sqlx::query_as::<_, ProcessLogRow>(
        r#"SELECT id as "id!: Uuid", session_id as "session_id!: Uuid"
           FROM execution_processes
           WHERE created_at < datetime('now', ?)
             AND status IN ('completed', 'failed', 'killed')"#,
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    let root = asset_dir();
    let mut deleted_files: u64 = 0;
    let mut bytes_freed: u64 = 0;

    for chunk in rows.chunks(PURGE_LOG_BATCH_SIZE) {
        let root = root.clone();
        let chunk_owned: Vec<(Uuid, Uuid)> = chunk.iter().map(|r| (r.session_id, r.id)).collect();
        let (chunk_deleted, chunk_bytes) = tokio::task::spawn_blocking(move || {
            let mut d: u64 = 0;
            let mut b: u64 = 0;
            for (session_id, process_id) in chunk_owned {
                let path = process_log_file_path_in_root(&root, session_id, process_id);
                if let Ok(metadata) = std::fs::metadata(&path) {
                    let len = metadata.len();
                    if std::fs::remove_file(&path).is_ok() {
                        d += 1;
                        b += len;
                    }
                }
            }
            (d, b)
        })
        .await
        .map_err(|e| ApiError::BadRequest(format!("purge_logs join error: {}", e)))?;

        deleted_files += chunk_deleted;
        bytes_freed += chunk_bytes;
        tokio::task::yield_now().await;
    }

    Ok(ResponseJson(ApiResponse::success(LogPurgeResult {
        deleted_files,
        bytes_freed,
    })))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/stats", get(get_stats))
        .route("/vacuum", post(vacuum))
        .route("/analyze", post(analyze))
        .route("/archived-stats", get(archived_stats))
        .route("/archived-non-terminal", get(archived_non_terminal))
        .route("/purge-archived", post(purge_archived))
        .route("/log-stats", get(log_stats))
        .route("/purge-logs", post(purge_logs))
}
