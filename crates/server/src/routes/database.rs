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

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedWorkspaceItem {
    pub id: Uuid,
    pub name: Option<String>,
    /// ISO-8601 string — the actual archive timestamp (archived_at if set, else updated_at)
    pub archived_at: String,
    /// ISO-8601 string
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArchivedListResponse {
    pub items: Vec<ArchivedWorkspaceItem>,
    pub older_than_days: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogSessionItem {
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_name: Option<String>,
    pub file_count: i64,
    pub total_bytes: i64,
    /// ISO-8601 date string of the oldest log file in this session.
    pub oldest_file_date: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogListResponse {
    pub items: Vec<LogSessionItem>,
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
    // Acquire the WRITE lock for the entire check+claim sequence.
    // This eliminates the TOCTOU race: two concurrent requests cannot both
    // pass the cooldown check because the second sees the timestamp set by
    // the first before either drops the lock.
    {
        let mut last = deployment.last_vacuum_time().write().await;
        if let Some(prev) = *last {
            let elapsed = Utc::now().signed_duration_since(prev).num_seconds();
            if elapsed < VACUUM_COOLDOWN_SECS {
                return Err(ApiError::TooManyRequests(
                    "Vacuum cooldown active".to_string(),
                ));
            }
        }
        // Claim the slot atomically. Set the timestamp *before* releasing the
        // lock so concurrent requests see the cooldown immediately. If VACUUM
        // fails below, the cooldown still applies — this prevents retry storms
        // on a DB that is under heavy load.
        *last = Some(Utc::now());
    } // write lock released here; VACUUM runs without holding any lock

    let pool = &deployment.db().pool;
    vacuum_database(pool)
        .await
        .map_err(|e| {
            tracing::error!("vacuum error: {e}");
            ApiError::Database(sqlx::Error::Protocol(e.to_string()))
        })
        .map(|result| ResponseJson(ApiResponse::success(result)))
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
           WHERE archived = 1
             AND COALESCE(archived_at, updated_at) < datetime('now', ?)"#,
    )
    .bind(&cutoff)
    .fetch_one(pool)
    .await?;

    Ok(ResponseJson(ApiResponse::success(ArchivedStatsResponse {
        count,
        older_than_days: query.older_than_days,
    })))
}

async fn archived_list(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<ArchivedListResponse>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }
    let pool = &deployment.db().pool;
    let cutoff = format!("-{} days", query.older_than_days);

    // Use sqlx::query (not macro) — avoids needing prepare-db for this query.
    let rows = sqlx::query(
        r#"SELECT
               id,
               name,
               COALESCE(archived_at, updated_at) AS effective_archived_at,
               created_at
           FROM workspaces
           WHERE archived = 1
             AND COALESCE(archived_at, updated_at) < datetime('now', ?)
             AND NOT EXISTS (
                 SELECT 1 FROM execution_processes ep
                 JOIN sessions s ON s.id = ep.session_id
                 WHERE s.workspace_id = workspaces.id
                   AND ep.status NOT IN ('completed', 'failed', 'killed')
             )
           ORDER BY COALESCE(archived_at, updated_at) ASC"#,
    )
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    use sqlx::Row;
    let items = rows
        .into_iter()
        .map(|r| {
            let id_str: String = r.get("id");
            ArchivedWorkspaceItem {
                id: Uuid::parse_str(&id_str).unwrap_or_default(),
                name: r.get("name"),
                archived_at: r
                    .get::<Option<String>, _>("effective_archived_at")
                    .unwrap_or_default(),
                created_at: r.get::<Option<String>, _>("created_at").unwrap_or_default(),
            }
        })
        .collect();

    Ok(ResponseJson(ApiResponse::success(ArchivedListResponse {
        items,
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
    let mut skipped_active: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM workspaces w
           WHERE w.archived = 1 AND COALESCE(w.archived_at, w.updated_at) < datetime('now', ?)
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
           WHERE w.archived = 1 AND COALESCE(w.archived_at, w.updated_at) < datetime('now', ?)
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
        // Re-check immediately before deletion: the workspace may have been
        // un-archived or may have acquired a new running process between our
        // initial fetch and now.
        let still_eligible: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1 FROM workspaces w
               WHERE w.id = ? AND w.archived = 1
                 AND NOT EXISTS (
                     SELECT 1 FROM execution_processes ep
                     JOIN sessions s ON s.id = ep.session_id
                     WHERE s.workspace_id = w.id
                       AND ep.status NOT IN ('completed', 'failed', 'killed')
                 )"#,
        )
        .bind(workspace.id)
        .fetch_optional(pool)
        .await?;

        if still_eligible.is_none() {
            tracing::warn!(
                workspace_id = %workspace.id,
                "Workspace no longer eligible for purge on re-check (un-archived or new active process); skipping"
            );
            skipped_active += 1;
            continue;
        }

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
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogPurgeResult>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    // Step 1: Collect candidate (process_id, path, size) in a blocking task.
    let candidates: Vec<(uuid::Uuid, std::path::PathBuf, u64)> =
        tokio::task::spawn_blocking(move || {
            let cutoff = std::time::SystemTime::now()
                - std::time::Duration::from_secs(older_than_days as u64 * 86400);
            collect_old_log_files(&log_root, cutoff)
        })
        .await
        .map_err(|e| {
            tracing::error!("purge_logs collect join error: {e}");
            ApiError::Database(sqlx::Error::Protocol(e.to_string()))
        })?;

    // Step 2: Filter out processes that are still running.
    // Orphaned files (process not in DB) are safe to delete.
    let pool = &deployment.db().pool;
    let mut safe_to_delete: Vec<(std::path::PathBuf, u64)> = Vec::new();
    for (process_id, path, size) in candidates {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM execution_processes WHERE id = ?")
                .bind(process_id.to_string())
                .fetch_optional(pool)
                .await?;

        match status.as_deref() {
            Some("running") => {
                tracing::debug!(
                    process_id = %process_id,
                    "Skipping log file for still-running process"
                );
            }
            _ => {
                // terminal status ('completed'/'failed'/'killed') OR orphaned (not in DB)
                safe_to_delete.push((path, size));
            }
        }
    }

    // Step 3: Delete safe files in a blocking task.
    let (deleted_files, bytes_freed) = tokio::task::spawn_blocking(move || {
        let mut deleted: i64 = 0;
        let mut freed: i64 = 0;
        for (path, size) in safe_to_delete {
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
        tracing::error!("purge_logs delete join error: {e}");
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
                    let mtime = match meta.modified() {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Cannot read mtime for log file — skipping conservatively"
                            );
                            continue;
                        }
                    };
                    if mtime < cutoff {
                        cb(&meta);
                    }
                }
            }
        }
    }
}

/// Collect log files older than `cutoff`, returning `(process_id, path, size_bytes)`.
///
/// The process UUID is extracted from the filename stem. Files whose stem is not
/// a valid UUID are skipped with a warning (they are unexpected and should not be deleted).
/// Files where `modified()` fails are skipped conservatively with a warning.
fn collect_old_log_files(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
) -> Vec<(uuid::Uuid, std::path::PathBuf, u64)> {
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
                // Extract process UUID from filename stem.
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let process_id = match uuid::Uuid::parse_str(stem) {
                    Ok(id) => id,
                    Err(_) => {
                        tracing::warn!(
                            path = %path.display(),
                            "Skipping log file with non-UUID filename"
                        );
                        continue;
                    }
                };
                let Ok(meta) = std::fs::metadata(&path) else {
                    continue;
                };
                let mtime = match meta.modified() {
                    Ok(t) => t,
                    Err(e) => {
                        // Conservative: treat as recent so we never accidentally
                        // delete a file whose age we cannot determine.
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Cannot read mtime for log file — skipping conservatively"
                        );
                        continue;
                    }
                };
                if mtime < cutoff {
                    result.push((process_id, path, meta.len()));
                }
            }
        }
    }
    result
}

/// Per-session summary of log files older than cutoff.
struct SessionLogSummary {
    session_id: Uuid,
    file_count: i64,
    total_bytes: i64,
    /// Oldest mtime found in this session (for sorting).
    oldest_mtime: std::time::SystemTime,
}

/// Walk log directories and return per-session summaries.
/// Sort and formatting are done by the caller.
fn collect_old_log_sessions(
    root: &std::path::Path,
    cutoff: std::time::SystemTime,
) -> Vec<SessionLogSummary> {
    let mut summaries: std::collections::HashMap<Uuid, SessionLogSummary> =
        std::collections::HashMap::new();

    let Ok(top) = std::fs::read_dir(root) else {
        return vec![];
    };
    for prefix_entry in top.flatten() {
        let Ok(sessions_dir) = std::fs::read_dir(prefix_entry.path()) else {
            continue;
        };
        for session_entry in sessions_dir.flatten() {
            let session_name = session_entry.file_name();
            let Some(name_str) = session_name.to_str() else {
                continue;
            };
            let Ok(session_id) = Uuid::parse_str(name_str) else {
                continue;
            };

            let processes_dir = session_entry.path().join("processes");
            let Ok(procs) = std::fs::read_dir(&processes_dir) else {
                continue;
            };

            for proc_entry in procs.flatten() {
                let path = proc_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(meta) = std::fs::metadata(&path) else {
                    continue;
                };
                let mtime = match meta.modified() {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Cannot read mtime — skipping log file"
                        );
                        continue;
                    }
                };
                if mtime < cutoff {
                    let entry = summaries
                        .entry(session_id)
                        .or_insert_with(|| SessionLogSummary {
                            session_id,
                            file_count: 0,
                            total_bytes: 0,
                            oldest_mtime: mtime,
                        });
                    entry.file_count += 1;
                    entry.total_bytes += meta.len() as i64;
                    if mtime < entry.oldest_mtime {
                        entry.oldest_mtime = mtime;
                    }
                }
            }
        }
    }
    summaries.into_values().collect()
}

async fn log_list(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<OlderThanQuery>,
) -> Result<ResponseJson<ApiResponse<LogListResponse>>, ApiError> {
    if query.older_than_days < 1 {
        return Err(ApiError::BadRequest(
            "older_than_days must be >= 1".to_string(),
        ));
    }

    let log_root = asset_dir().join(EXECUTION_LOGS_DIRNAME);
    let older_than_days = query.older_than_days;

    // Step 1: Collect per-session summaries in a blocking task.
    let mut session_summaries: Vec<SessionLogSummary> = tokio::task::spawn_blocking(move || {
        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(older_than_days as u64 * 86400);
        collect_old_log_sessions(&log_root, cutoff)
    })
    .await
    .map_err(|e| ApiError::Database(sqlx::Error::Protocol(e.to_string())))?;

    if session_summaries.is_empty() {
        return Ok(ResponseJson(ApiResponse::success(LogListResponse {
            items: vec![],
            older_than_days: query.older_than_days,
        })));
    }

    // Sort by oldest_mtime ascending (SystemTime comparison, NOT string comparison).
    session_summaries.sort_by(|a, b| a.oldest_mtime.cmp(&b.oldest_mtime));

    // Step 2: Batch DB lookup — get workspace info for all session IDs at once.
    // SQLite doesn't support array params, so build an IN clause manually.
    // Chunk at 999 to stay under SQLITE_LIMIT_VARIABLE_NUMBER (default 999).
    const SQLITE_MAX_VARS: usize = 999;
    let pool = &deployment.db().pool;
    let session_id_strings: Vec<String> = session_summaries
        .iter()
        .map(|s| s.session_id.to_string())
        .collect();

    use sqlx::Row;
    let mut session_map: std::collections::HashMap<String, (Uuid, Option<String>)> =
        std::collections::HashMap::new();

    for chunk in session_id_strings.chunks(SQLITE_MAX_VARS) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            r#"SELECT s.id AS session_id, s.workspace_id, w.name AS workspace_name
               FROM sessions s
               JOIN workspaces w ON w.id = s.workspace_id
               WHERE s.id IN ({})"#,
            placeholders
        );
        let mut query_builder = sqlx::query(&sql);
        for id in chunk {
            query_builder = query_builder.bind(id);
        }
        let rows = query_builder.fetch_all(pool).await?;
        for row in rows {
            let session_id_str: String = row.get("session_id");
            let workspace_id_str: String = row.get("workspace_id");
            let workspace_name: Option<String> = row.get("workspace_name");
            let workspace_id = Uuid::parse_str(&workspace_id_str).unwrap_or_default();
            session_map.insert(session_id_str, (workspace_id, workspace_name));
        }
    }

    // Step 3: Build response items (already sorted by oldest_mtime).
    let items: Vec<LogSessionItem> = session_summaries
        .into_iter()
        .filter_map(|summary| {
            let session_id_str = summary.session_id.to_string();
            let (workspace_id, workspace_name) = session_map.get(&session_id_str)?.clone();

            // Convert oldest_mtime to ISO-8601 date string.
            let oldest_file_date = {
                let secs = summary
                    .oldest_mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let dt = chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
                dt.format("%Y-%m-%d").to_string()
            };

            Some(LogSessionItem {
                session_id: summary.session_id,
                workspace_id,
                workspace_name,
                file_count: summary.file_count,
                total_bytes: summary.total_bytes,
                oldest_file_date,
            })
        })
        .collect();

    Ok(ResponseJson(ApiResponse::success(LogListResponse {
        items,
        older_than_days: query.older_than_days,
    })))
}

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
        .route("/database/archived-list", get(archived_list))
        .route("/database/log-list", get(log_list))
}
