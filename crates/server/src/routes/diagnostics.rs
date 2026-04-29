use axum::{
    Router,
    extract::State,
    response::Json as ResponseJson,
    routing::get,
};
use db::{
    database_stats::{DatabaseStats, get_database_stats},
    metrics::PoolStats,
    models::workspace::Workspace,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use deployment::Deployment;
use utils::{assets::asset_dir, response::ApiResponse};

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiagnosticsResponse {
    pub pool_stats: PoolStats,
    pub database_stats: DatabaseStats,
    pub wal_size_bytes: u64,
    pub wal_size_human: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceDiskUsage {
    pub workspace_id: Uuid,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskUsageResponse {
    pub workspaces: Vec<WorkspaceDiskUsage>,
    pub total_bytes: u64,
    pub total_human: String,
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    match bytes {
        b if b >= GB => format!("{:.1} GB", b as f64 / GB as f64),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.1} KB", b as f64 / KB as f64),
        b => format!("{} B", b),
    }
}

async fn get_diagnostics(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DiagnosticsResponse>>, ApiError> {
    let pool = &deployment.db().pool;
    let db_path = asset_dir().join("db.v2.sqlite");

    let pool_stats = deployment.db().pool_stats();
    let database_stats = get_database_stats(pool, &db_path)
        .await
        .map_err(|e| ApiError::Database(sqlx::Error::Protocol(e.to_string())))?;

    let wal_size_bytes = database_stats.wal_size_bytes as u64;
    let wal_size_human = format_bytes(wal_size_bytes);

    Ok(ResponseJson(ApiResponse::success(DiagnosticsResponse {
        pool_stats,
        database_stats,
        wal_size_bytes,
        wal_size_human,
    })))
}

async fn get_disk_usage(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<DiskUsageResponse>>, ApiError> {
    let workspaces: Vec<Workspace> = sqlx::query_as(
        "SELECT * FROM workspaces WHERE container_ref IS NOT NULL AND worktree_deleted = 0",
    )
    .fetch_all(&deployment.db().pool)
    .await
    .map_err(ApiError::Database)?;

    let mut usage_list: Vec<WorkspaceDiskUsage> = Vec::new();

    for workspace in workspaces {
        let Some(container_ref) = workspace.container_ref else {
            continue;
        };
        let path = std::path::PathBuf::from(&container_ref);

        if !path.exists() {
            continue;
        }

        let size_bytes = tokio::task::spawn_blocking({
            let path = path.clone();
            move || {
                let mut total = 0u64;
                let mut stack = vec![path];
                while let Some(dir) = stack.pop() {
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            if let Ok(meta) = entry.metadata() {
                                if meta.is_file() {
                                    total += meta.len();
                                } else if meta.is_dir() {
                                    stack.push(entry.path());
                                }
                            }
                        }
                    }
                }
                total
            }
        })
        .await
        .unwrap_or(0);

        usage_list.push(WorkspaceDiskUsage {
            workspace_id: workspace.id,
            path: container_ref,
            size_bytes,
        });
    }

    usage_list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    usage_list.truncate(50);

    let total_bytes: u64 = usage_list.iter().map(|w| w.size_bytes).sum();
    let total_human = format_bytes(total_bytes);

    Ok(ResponseJson(ApiResponse::success(DiskUsageResponse {
        workspaces: usage_list,
        total_bytes,
        total_human,
    })))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/diagnostics", get(get_diagnostics))
        .route("/diagnostics/disk-usage", get(get_disk_usage))
}
