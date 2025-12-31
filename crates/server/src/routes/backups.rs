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

/// Restore database from an uploaded backup file
async fn restore_backup(
    mut multipart: Multipart,
) -> Result<ResponseJson<ApiResponse<String>>, ApiError> {
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("backup") {
            let data = field.bytes().await?;
            let db_path = database_path();
            BackupService::restore_from_data(&db_path, &data)?;
            return Ok(ResponseJson(ApiResponse::success(
                "You must restart the application for the database restore to finalise."
                    .to_string(),
            )));
        }
    }
    Err(ApiError::BadRequest("No backup file provided".into()))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/backups", get(list_backups).post(create_backup))
        .route(
            "/backups/restore",
            post(restore_backup).layer(DefaultBodyLimit::max(500 * 1024 * 1024)), // 500MB limit
        )
        .route("/backups/{filename}", delete(delete_backup))
        .route("/backups/{filename}/download", get(download_backup))
}
