use api_types::{
    AttachmentUrlResponse, AttachmentWithBlob, AttachmentWithUrl, ListAttachmentsResponse,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::instrument;
use ts_rs::TS;
use uuid::Uuid;

use super::organization_members::{
    ensure_comment_access, ensure_issue_access, ensure_project_access,
};
use crate::{
    AppState,
    attachments::thumbnail::ThumbnailService,
    auth::RequestContext,
    azure_blob::AzureBlobError,
    db::{
        attachments::{AttachmentError, AttachmentRepository},
        blobs::{BlobError, BlobRepository},
        pending_uploads::{PendingUploadError, PendingUploadRepository},
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attachments/init", post(init_upload))
        .route("/attachments/confirm", post(confirm_upload))
        .route("/attachments/{id}/file", get(get_attachment_file))
        .route("/attachments/{id}/thumbnail", get(get_attachment_thumbnail))
        .route("/attachments/{id}", delete(delete_attachment))
        .route(
            "/issues/{issue_id}/attachments",
            get(list_issue_attachments),
        )
        .route(
            "/issues/{issue_id}/attachments/commit",
            post(commit_issue_attachments),
        )
        .route(
            "/comments/{comment_id}/attachments",
            get(list_comment_attachments),
        )
        .route(
            "/comments/{comment_id}/attachments/commit",
            post(commit_comment_attachments),
        )
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct InitUploadRequest {
    pub project_id: Uuid,
    pub filename: String,
    #[ts(type = "number")]
    pub size_bytes: i64,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct InitUploadResponse {
    pub upload_url: String,
    pub upload_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub skip_upload: bool,
    pub existing_blob_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ConfirmUploadRequest {
    pub project_id: Uuid,
    pub upload_id: Uuid,
    pub filename: String,
    #[ts(optional)]
    pub content_type: Option<String>,
    #[ts(type = "number")]
    pub size_bytes: i64,
    pub hash: String,
    #[ts(optional)]
    pub issue_id: Option<Uuid>,
    #[ts(optional)]
    pub comment_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct CommitAttachmentsRequest {
    pub attachment_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct CommitAttachmentsResponse {
    pub attachments: Vec<AttachmentWithBlob>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Azure Blob storage not configured")]
    NotConfigured,
    #[error("Azure Blob error: {0}")]
    AzureBlob(#[from] AzureBlobError),
    #[error("attachment error: {0}")]
    Attachment(#[from] AttachmentError),
    #[error("blob error: {0}")]
    Blob(#[from] BlobError),
    #[error("attachment not found")]
    NotFound,
    #[error("no thumbnail available")]
    NoThumbnail,
    #[error("access denied")]
    AccessDenied,
    #[error("file too large (max 20MB)")]
    FileTooLarge,
    #[error("invalid upload")]
    InvalidUpload,
    #[error("upload not found or expired")]
    UploadNotFound,
    #[error("pending upload error: {0}")]
    PendingUpload(#[from] PendingUploadError),
    #[error("thumbnail generation failed: {0}")]
    ThumbnailError(String),
}

impl IntoResponse for RouteError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            RouteError::NotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Attachment storage not available",
            ),
            RouteError::AzureBlob(e) => {
                tracing::error!(error = %e, "Azure Blob error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Storage error")
            }
            RouteError::Attachment(e) => {
                tracing::error!(error = %e, "Attachment error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            RouteError::Blob(e) => {
                tracing::error!(error = %e, "Blob error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            RouteError::NotFound => (StatusCode::NOT_FOUND, "Attachment not found"),
            RouteError::NoThumbnail => (StatusCode::NOT_FOUND, "No thumbnail available"),
            RouteError::AccessDenied => (StatusCode::FORBIDDEN, "Access denied"),
            RouteError::FileTooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, "File too large (max 20MB)")
            }
            RouteError::InvalidUpload => (StatusCode::BAD_REQUEST, "Invalid upload"),
            RouteError::UploadNotFound => (StatusCode::NOT_FOUND, "Upload not found or expired"),
            RouteError::PendingUpload(e) => {
                tracing::error!(error = %e, "Pending upload error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            RouteError::ThumbnailError(e) => {
                tracing::error!(error = %e, "Thumbnail generation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Thumbnail generation failed",
                )
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

const MAX_FILE_SIZE: i64 = 20 * 1024 * 1024;

fn validate_client_upload_size(size_bytes: i64) -> Result<(), RouteError> {
    if size_bytes < 0 {
        return Err(RouteError::InvalidUpload);
    }
    if size_bytes > MAX_FILE_SIZE {
        return Err(RouteError::FileTooLarge);
    }
    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn validate_confirm_upload(
    payload: &ConfirmUploadRequest,
    pending: &crate::db::pending_uploads::PendingUpload,
    content_length: i64,
    blob_data: &[u8],
    now: DateTime<Utc>,
) -> Result<i64, RouteError> {
    validate_client_upload_size(payload.size_bytes)?;

    if pending.project_id != payload.project_id || pending.expires_at <= now {
        return Err(RouteError::UploadNotFound);
    }

    if pending.hash != payload.hash {
        return Err(RouteError::InvalidUpload);
    }

    if content_length < 0 {
        return Err(RouteError::InvalidUpload);
    }
    if content_length > MAX_FILE_SIZE {
        return Err(RouteError::FileTooLarge);
    }

    let server_hash = sha256_hex(blob_data);
    if server_hash != pending.hash {
        return Err(RouteError::InvalidUpload);
    }

    Ok(content_length)
}

#[instrument(name = "attachments.init_upload", skip(state, ctx, payload), fields(project_id = %payload.project_id, user_id = %ctx.user.id))]
async fn init_upload(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<InitUploadRequest>,
) -> Result<Json<InitUploadResponse>, RouteError> {
    ensure_project_access(state.pool(), ctx.user.id, payload.project_id)
        .await
        .map_err(|_| RouteError::AccessDenied)?;

    validate_client_upload_size(payload.size_bytes)?;

    if let Some(existing) =
        BlobRepository::find_by_hash(state.pool(), payload.project_id, &payload.hash).await?
    {
        let azure = state.azure_blob().ok_or(RouteError::NotConfigured)?;
        let read_url = azure.create_read_url(&existing.blob_path)?;

        return Ok(Json(InitUploadResponse {
            upload_url: read_url,
            upload_id: existing.id,
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            skip_upload: true,
            existing_blob_id: Some(existing.id),
        }));
    }

    let azure = state.azure_blob().ok_or(RouteError::NotConfigured)?;
    let sanitized_filename = sanitize_filename(&payload.filename);
    let blob_path = format!(
        "attachments/{}/{}_{}",
        payload.project_id,
        Uuid::new_v4(),
        sanitized_filename
    );
    let upload = azure.create_upload_url(&blob_path)?;

    let pending = PendingUploadRepository::create(
        state.pool(),
        payload.project_id,
        upload.blob_path,
        payload.hash.clone(),
        upload.expires_at,
    )
    .await?;

    Ok(Json(InitUploadResponse {
        upload_url: upload.upload_url,
        upload_id: pending.id,
        expires_at: upload.expires_at,
        skip_upload: false,
        existing_blob_id: None,
    }))
}

#[instrument(name = "attachments.confirm_upload", skip(state, ctx, payload), fields(project_id = %payload.project_id, user_id = %ctx.user.id))]
async fn confirm_upload(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<ConfirmUploadRequest>,
) -> Result<Json<AttachmentWithBlob>, RouteError> {
    ensure_project_access(state.pool(), ctx.user.id, payload.project_id)
        .await
        .map_err(|_| RouteError::AccessDenied)?;

    if let Some(issue_id) = payload.issue_id {
        ensure_issue_access(state.pool(), ctx.user.id, issue_id)
            .await
            .map_err(|_| RouteError::AccessDenied)?;
    }
    if let Some(comment_id) = payload.comment_id {
        ensure_comment_access(state.pool(), ctx.user.id, comment_id)
            .await
            .map_err(|_| RouteError::AccessDenied)?;
    }
    validate_client_upload_size(payload.size_bytes)?;

    let azure = state.azure_blob().ok_or(RouteError::NotConfigured)?;

    let blob = if let Some(existing) =
        BlobRepository::find_by_hash(state.pool(), payload.project_id, &payload.hash).await?
    {
        existing
    } else {
        let pending = PendingUploadRepository::find_by_id(state.pool(), payload.upload_id)
            .await?
            .ok_or(RouteError::UploadNotFound)?;

        let blob_path = &pending.blob_path;

        let props = azure.get_blob_properties(blob_path).await?;
        if props.content_length < 0 {
            return Err(RouteError::InvalidUpload);
        }
        if props.content_length > MAX_FILE_SIZE {
            let _ = azure.delete_blob(blob_path).await;
            return Err(RouteError::FileTooLarge);
        }

        let blob_data = azure.download_blob(blob_path).await?;
        let actual_size_bytes = match validate_confirm_upload(
            &payload,
            &pending,
            props.content_length,
            &blob_data,
            Utc::now(),
        ) {
            Ok(size_bytes) => size_bytes,
            Err(err @ RouteError::FileTooLarge) => {
                let _ = azure.delete_blob(blob_path).await;
                return Err(err);
            }
            Err(err) => return Err(err),
        };

        let thumbnail_result =
            ThumbnailService::generate(&blob_data, payload.content_type.as_deref())
                .map_err(|e| RouteError::ThumbnailError(e.to_string()))?;

        let _ = PendingUploadRepository::delete(state.pool(), pending.id).await;

        let (thumbnail_blob_path, width, height) = match thumbnail_result {
            Some(thumb) => {
                let thumb_path = format!("thumbnails/{}", blob_path);
                azure
                    .upload_blob(&thumb_path, thumb.bytes, thumb.mime_type)
                    .await?;
                (
                    Some(thumb_path),
                    Some(thumb.original_width as i32),
                    Some(thumb.original_height as i32),
                )
            }
            None => (None, None, None),
        };

        BlobRepository::create(
            state.pool(),
            None,
            payload.project_id,
            blob_path.clone(),
            thumbnail_blob_path,
            payload.filename.clone(),
            payload.content_type.clone(),
            actual_size_bytes,
            pending.hash.clone(),
            width,
            height,
        )
        .await?
    };

    let expires_at = if payload.issue_id.is_some() || payload.comment_id.is_some() {
        None
    } else {
        Some(Utc::now() + chrono::Duration::hours(24))
    };

    let attachment = AttachmentRepository::create(
        state.pool(),
        None,
        blob.id,
        payload.issue_id,
        payload.comment_id,
        expires_at,
    )
    .await?;

    let result = AttachmentRepository::find_by_id_with_blob(state.pool(), attachment.id)
        .await?
        .ok_or(RouteError::NotFound)?;

    Ok(Json(result))
}

#[instrument(name = "attachments.commit_issue", skip(state, ctx, payload), fields(issue_id = %issue_id, user_id = %ctx.user.id))]
async fn commit_issue_attachments(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(issue_id): Path<Uuid>,
    Json(payload): Json<CommitAttachmentsRequest>,
) -> Result<Json<CommitAttachmentsResponse>, RouteError> {
    ensure_issue_access(state.pool(), ctx.user.id, issue_id)
        .await
        .map_err(|_| RouteError::AccessDenied)?;

    let attachments =
        AttachmentRepository::commit_to_issue(state.pool(), &payload.attachment_ids, issue_id)
            .await?;
    Ok(Json(CommitAttachmentsResponse { attachments }))
}

#[instrument(name = "attachments.commit_comment", skip(state, ctx, payload), fields(comment_id = %comment_id, user_id = %ctx.user.id))]
async fn commit_comment_attachments(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(comment_id): Path<Uuid>,
    Json(payload): Json<CommitAttachmentsRequest>,
) -> Result<Json<CommitAttachmentsResponse>, RouteError> {
    ensure_comment_access(state.pool(), ctx.user.id, comment_id)
        .await
        .map_err(|_| RouteError::AccessDenied)?;

    let attachments =
        AttachmentRepository::commit_to_comment(state.pool(), &payload.attachment_ids, comment_id)
            .await?;
    Ok(Json(CommitAttachmentsResponse { attachments }))
}

#[instrument(name = "attachments.list_issue", skip(state, ctx), fields(issue_id = %issue_id, user_id = %ctx.user.id))]
async fn list_issue_attachments(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(issue_id): Path<Uuid>,
) -> Result<Json<ListAttachmentsResponse>, RouteError> {
    ensure_issue_access(state.pool(), ctx.user.id, issue_id)
        .await
        .map_err(|_| RouteError::AccessDenied)?;

    let azure = state.azure_blob();
    let attachments = AttachmentRepository::find_by_issue_id(state.pool(), issue_id)
        .await?
        .into_iter()
        .map(|a| {
            let file_url = azure.and_then(|az| az.create_read_url(&a.blob_path).ok());
            AttachmentWithUrl {
                attachment: a,
                file_url,
            }
        })
        .collect();
    Ok(Json(ListAttachmentsResponse { attachments }))
}

#[instrument(name = "attachments.list_comment", skip(state, ctx), fields(comment_id = %comment_id, user_id = %ctx.user.id))]
async fn list_comment_attachments(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(comment_id): Path<Uuid>,
) -> Result<Json<ListAttachmentsResponse>, RouteError> {
    ensure_comment_access(state.pool(), ctx.user.id, comment_id)
        .await
        .map_err(|_| RouteError::AccessDenied)?;

    let azure = state.azure_blob();
    let attachments = AttachmentRepository::find_by_comment_id(state.pool(), comment_id)
        .await?
        .into_iter()
        .map(|a| {
            let file_url = azure.and_then(|az| az.create_read_url(&a.blob_path).ok());
            AttachmentWithUrl {
                attachment: a,
                file_url,
            }
        })
        .collect();
    Ok(Json(ListAttachmentsResponse { attachments }))
}

#[instrument(name = "attachments.get_file", skip(state, ctx), fields(attachment_id = %id, user_id = %ctx.user.id))]
async fn get_attachment_file(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<AttachmentUrlResponse>, RouteError> {
    let attachment = AttachmentRepository::find_by_id_with_blob(state.pool(), id)
        .await?
        .ok_or(RouteError::NotFound)?;

    ensure_attachment_access(&state, ctx.user.id, &attachment).await?;

    let azure = state.azure_blob().ok_or(RouteError::NotConfigured)?;
    let url = azure.create_read_url(&attachment.blob_path)?;
    Ok(Json(AttachmentUrlResponse { url }))
}

#[instrument(name = "attachments.get_thumbnail", skip(state, ctx), fields(attachment_id = %id, user_id = %ctx.user.id))]
async fn get_attachment_thumbnail(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<AttachmentUrlResponse>, RouteError> {
    let attachment = AttachmentRepository::find_by_id_with_blob(state.pool(), id)
        .await?
        .ok_or(RouteError::NotFound)?;

    ensure_attachment_access(&state, ctx.user.id, &attachment).await?;

    let thumbnail_path = attachment
        .thumbnail_blob_path
        .ok_or(RouteError::NoThumbnail)?;
    let azure = state.azure_blob().ok_or(RouteError::NotConfigured)?;
    let url = azure.create_read_url(&thumbnail_path)?;
    Ok(Json(AttachmentUrlResponse { url }))
}

#[instrument(name = "attachments.delete", skip(state, ctx), fields(attachment_id = %id, user_id = %ctx.user.id))]
async fn delete_attachment(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, RouteError> {
    let attachment = AttachmentRepository::find_by_id_with_blob(state.pool(), id)
        .await?
        .ok_or(RouteError::NotFound)?;

    ensure_attachment_access(&state, ctx.user.id, &attachment).await?;

    let blob_id = attachment.blob_id;
    AttachmentRepository::delete(state.pool(), id).await?;

    if let Some(blob) = BlobRepository::delete_if_unreferenced(state.pool(), blob_id).await? {
        let azure = state.azure_blob().ok_or(RouteError::NotConfigured)?;
        if let Err(e) = azure.delete_blob(&blob.blob_path).await {
            tracing::warn!(error = %e, blob_path = %blob.blob_path, "Failed to delete blob");
        }
        if let Some(thumb_path) = blob.thumbnail_blob_path
            && let Err(e) = azure.delete_blob(&thumb_path).await
        {
            tracing::warn!(error = %e, blob_path = %thumb_path, "Failed to delete thumbnail");
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_attachment_access(
    state: &AppState,
    user_id: Uuid,
    attachment: &AttachmentWithBlob,
) -> Result<(), RouteError> {
    if let Some(issue_id) = attachment.issue_id {
        ensure_issue_access(state.pool(), user_id, issue_id)
            .await
            .map_err(|_| RouteError::AccessDenied)?;
    } else if let Some(comment_id) = attachment.comment_id {
        ensure_comment_access(state.pool(), user_id, comment_id)
            .await
            .map_err(|_| RouteError::AccessDenied)?;
    } else if let Some(project_id) =
        AttachmentRepository::project_id(state.pool(), attachment.id).await?
    {
        ensure_project_access(state.pool(), user_id, project_id)
            .await
            .map_err(|_| RouteError::AccessDenied)?;
    } else {
        return Err(RouteError::AccessDenied);
    }
    Ok(())
}

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::{ConfirmUploadRequest, MAX_FILE_SIZE, RouteError, validate_confirm_upload};
    use crate::db::pending_uploads::PendingUpload;

    fn pending_upload(project_id: uuid::Uuid, hash: String) -> PendingUpload {
        PendingUpload {
            id: uuid::Uuid::new_v4(),
            project_id,
            blob_path: "attachments/blob".to_string(),
            hash,
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(5),
        }
    }

    fn confirm_request(
        project_id: uuid::Uuid,
        upload_id: uuid::Uuid,
        hash: String,
        size_bytes: i64,
    ) -> ConfirmUploadRequest {
        ConfirmUploadRequest {
            project_id,
            upload_id,
            filename: "file.txt".to_string(),
            content_type: Some("text/plain".to_string()),
            size_bytes,
            hash,
            issue_id: None,
            comment_id: None,
        }
    }

    #[test]
    fn confirm_upload_rejects_project_mismatch() {
        let project_id = uuid::Uuid::new_v4();
        let other_project_id = uuid::Uuid::new_v4();
        let data = b"hello";
        let hash = super::sha256_hex(data);
        let pending = pending_upload(project_id, hash.clone());
        let payload = confirm_request(other_project_id, pending.id, hash, data.len() as i64);

        let result =
            validate_confirm_upload(&payload, &pending, data.len() as i64, data, Utc::now());

        assert!(matches!(result, Err(RouteError::UploadNotFound)));
    }

    #[test]
    fn confirm_upload_rejects_expired_uploads() {
        let project_id = uuid::Uuid::new_v4();
        let data = b"hello";
        let hash = super::sha256_hex(data);
        let mut pending = pending_upload(project_id, hash.clone());
        pending.expires_at = Utc::now() - Duration::seconds(1);
        let payload = confirm_request(project_id, pending.id, hash, data.len() as i64);

        let result =
            validate_confirm_upload(&payload, &pending, data.len() as i64, data, Utc::now());

        assert!(matches!(result, Err(RouteError::UploadNotFound)));
    }

    #[test]
    fn confirm_upload_rejects_client_hash_mismatch() {
        let project_id = uuid::Uuid::new_v4();
        let data = b"hello";
        let pending = pending_upload(project_id, super::sha256_hex(data));
        let payload = confirm_request(
            project_id,
            pending.id,
            "wrong".to_string(),
            data.len() as i64,
        );

        let result =
            validate_confirm_upload(&payload, &pending, data.len() as i64, data, Utc::now());

        assert!(matches!(result, Err(RouteError::InvalidUpload)));
    }

    #[test]
    fn confirm_upload_rejects_server_hash_mismatch() {
        let project_id = uuid::Uuid::new_v4();
        let hash = super::sha256_hex(b"expected");
        let pending = pending_upload(project_id, hash.clone());
        let payload = confirm_request(project_id, pending.id, hash, 5);

        let result = validate_confirm_upload(&payload, &pending, 5, b"wrong", Utc::now());

        assert!(matches!(result, Err(RouteError::InvalidUpload)));
    }

    #[test]
    fn confirm_upload_rejects_negative_and_oversized_client_sizes() {
        let project_id = uuid::Uuid::new_v4();
        let data = b"hello";
        let hash = super::sha256_hex(data);
        let pending = pending_upload(project_id, hash.clone());
        let negative = confirm_request(project_id, pending.id, hash.clone(), -1);
        let oversized = confirm_request(project_id, pending.id, hash, MAX_FILE_SIZE + 1);

        assert!(matches!(
            validate_confirm_upload(&negative, &pending, data.len() as i64, data, Utc::now()),
            Err(RouteError::InvalidUpload)
        ));
        assert!(matches!(
            validate_confirm_upload(&oversized, &pending, data.len() as i64, data, Utc::now()),
            Err(RouteError::FileTooLarge)
        ));
    }

    #[test]
    fn confirm_upload_returns_actual_blob_size() {
        let project_id = uuid::Uuid::new_v4();
        let data = b"hello";
        let hash = super::sha256_hex(data);
        let pending = pending_upload(project_id, hash.clone());
        let payload = confirm_request(project_id, pending.id, hash, 1);

        let actual_size =
            validate_confirm_upload(&payload, &pending, data.len() as i64, data, Utc::now())
                .expect("valid upload should return actual blob size");

        assert_eq!(actual_size, data.len() as i64);
    }
}
