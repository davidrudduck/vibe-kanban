use axum::{Router, response::Json as ResponseJson, routing::get};
use db::{BackupInfo, BackupService};
use utils::{assets::asset_dir, response::ApiResponse};

use crate::{DeploymentImpl, error::ApiError};

/// List all available database backups
async fn list_backups() -> Result<ResponseJson<ApiResponse<Vec<BackupInfo>>>, ApiError> {
    let db_path = asset_dir().join("db.sqlite");
    let backups = BackupService::list_backups(&db_path)?;
    Ok(ResponseJson(ApiResponse::success(backups)))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/backups", get(list_backups))
}
