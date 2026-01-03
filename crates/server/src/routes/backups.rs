use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path},
    http::Response,
    response::Json as ResponseJson,
    routing::{delete, get, post},
};
use db::{BackupInfo, BackupService};
use tokio_util::io::ReaderStream;
use utils::{assets::database_path, response::ApiResponse};

use crate::{DeploymentImpl, error::ApiError};

/// List all available database backups
async fn list_backups() -> Result<ResponseJson<ApiResponse<Vec<BackupInfo>>>, ApiError> {
    let db_path = database_path();
    let backups = BackupService::list_backups(&db_path)?;
    Ok(ResponseJson(ApiResponse::success(backups)))
}

/// Create a new database backup
async fn create_backup() -> Result<ResponseJson<ApiResponse<BackupInfo>>, ApiError> {
    let db_path = database_path();
    let info = BackupService::create_backup(&db_path)?;
    Ok(ResponseJson(ApiResponse::success(info)))
}

/// Delete a database backup by filename
async fn delete_backup(
    Path(filename): Path<String>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let db_path = database_path();
    BackupService::delete_backup(&db_path, &filename)?;
    Ok(ResponseJson(ApiResponse::success(())))
}

/// Download a database backup file
async fn download_backup(Path(filename): Path<String>) -> Result<Response<Body>, ApiError> {
    let db_path = database_path();
    let backup_path = BackupService::get_backup_path(&db_path, &filename)?;

    let file = tokio::fs::File::open(&backup_path).await?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .unwrap())
}

/// Restore database from an uploaded backup file.
///
/// IMPORTANT: This stages the backup for restore on next server restart.
/// The actual restore happens during server startup, BEFORE the database pool is created.
/// This prevents corruption from overwriting the database while connections are active.
async fn restore_backup(
    mut multipart: Multipart,
) -> Result<ResponseJson<ApiResponse<RestoreResponse>>, ApiError> {
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("backup") {
            let data = field.bytes().await?;
            let db_path = database_path();

            // Stage the backup for restore instead of overwriting directly
            // This is safe because the actual restore happens on next startup
            BackupService::stage_restore(&db_path, &data)?;

            return Ok(ResponseJson(ApiResponse::success(RestoreResponse {
                message: "Backup staged for restore. Please restart the server to complete the restore.".to_string(),
                restart_required: true,
            })));
        }
    }
    Err(ApiError::BadRequest("No backup file provided".into()))
}

/// Check if a database restore is pending.
async fn restore_status() -> Result<ResponseJson<ApiResponse<RestoreStatusResponse>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(RestoreStatusResponse {
        pending: BackupService::is_restore_pending(),
    })))
}

/// Response from the restore endpoint.
#[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct RestoreResponse {
    pub message: String,
    pub restart_required: bool,
}

/// Response from the restore status endpoint.
#[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct RestoreStatusResponse {
    pub pending: bool,
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/backups", get(list_backups).post(create_backup))
        .route(
            "/backups/restore",
            post(restore_backup).layer(DefaultBodyLimit::max(500 * 1024 * 1024)), // 500MB limit
        )
        .route("/backups/restore/status", get(restore_status))
        .route("/backups/{filename}", delete(delete_backup))
        .route("/backups/{filename}/download", get(download_backup))
}
