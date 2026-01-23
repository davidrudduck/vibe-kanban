use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::warn;
use utils::api::organizations::{
    ListMembersResponse, OrganizationMemberWithProfile, RevokeInvitationRequest,
    UpdateMemberRoleRequest, UpdateMemberRoleResponse,
};
use uuid::Uuid;

use super::error::{ErrorResponse, membership_error};
use crate::{
    AppState,
    auth::RequestContext,
    db::{
        identity_errors::IdentityError,
        invitations::{Invitation, InvitationRepository},
        node_projects::NodeProjectRepository,
        organization_members::{self, MemberRole},
        organizations::OrganizationRepository,
        swarm_projects::SwarmProjectRepository,
        tasks::SharedTaskRepository,
    },
};

pub fn public_router() -> Router<AppState> {
    Router::new().route("/invitations/{token}", get(get_invitation))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route(
            "/organizations/{org_id}/invitations",
            post(create_invitation),
        )
        .route("/organizations/{org_id}/invitations", get(list_invitations))
        .route(
            "/organizations/{org_id}/invitations/revoke",
            post(revoke_invitation),
        )
        .route("/invitations/{token}/accept", post(accept_invitation))
        .route("/organizations/{org_id}/members", get(list_members))
        .route(
            "/organizations/{org_id}/members/{user_id}",
            delete(remove_member),
        )
        .route(
            "/organizations/{org_id}/members/{user_id}/role",
            patch(update_member_role),
        )
}

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: MemberRole,
}

#[derive(Debug, Serialize)]
pub struct CreateInvitationResponse {
    pub invitation: Invitation,
}

#[derive(Debug, Serialize)]
pub struct ListInvitationsResponse {
    pub invitations: Vec<Invitation>,
}

#[derive(Debug, Serialize)]
pub struct GetInvitationResponse {
    pub id: Uuid,
    pub organization_slug: String,
    pub organization_name: String,
    pub role: MemberRole,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AcceptInvitationResponse {
    pub organization_id: String,
    pub organization_slug: String,
    pub role: MemberRole,
}

pub async fn create_invitation(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
    Json(payload): Json<CreateInvitationRequest>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    let org_repo = OrganizationRepository::new(&state.pool);
    let invitation_repo = InvitationRepository::new(&state.pool);

    ensure_admin_access(&state.pool, org_id, user.id).await?;

    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::days(7);

    let invitation = invitation_repo
        .create_invitation(
            org_id,
            user.id,
            &payload.email,
            payload.role,
            expires_at,
            &token,
        )
        .await
        .map_err(|e| match e {
            IdentityError::PermissionDenied => {
                ErrorResponse::new(StatusCode::FORBIDDEN, "Admin access required")
            }
            IdentityError::InvitationError(msg) => ErrorResponse::new(StatusCode::BAD_REQUEST, msg),
            _ => ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
        })?;

    let organization = org_repo.fetch_organization(org_id).await.map_err(|_| {
        ErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch organization",
        )
    })?;

    let accept_url = format!(
        "{}/invitations/{}/accept",
        state.server_public_base_url, token
    );
    state
        .mailer
        .send_org_invitation(
            &organization.name,
            &payload.email,
            &accept_url,
            payload.role,
            user.username.as_deref(),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(CreateInvitationResponse { invitation }),
    ))
}

pub async fn list_invitations(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    let invitation_repo = InvitationRepository::new(&state.pool);

    ensure_admin_access(&state.pool, org_id, user.id).await?;

    let invitations = invitation_repo
        .list_invitations(org_id, user.id)
        .await
        .map_err(|e| match e {
            IdentityError::PermissionDenied => {
                ErrorResponse::new(StatusCode::FORBIDDEN, "Admin access required")
            }
            IdentityError::InvitationError(msg) => ErrorResponse::new(StatusCode::BAD_REQUEST, msg),
            _ => ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
        })?;

    Ok(Json(ListInvitationsResponse { invitations }))
}

pub async fn get_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let invitation_repo = InvitationRepository::new(&state.pool);

    let invitation = invitation_repo
        .get_invitation_by_token(&token)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, "Invitation not found"))?;

    let org_repo = OrganizationRepository::new(&state.pool);
    let org = org_repo
        .fetch_organization(invitation.organization_id)
        .await
        .map_err(|_| {
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch organization",
            )
        })?;

    Ok(Json(GetInvitationResponse {
        id: invitation.id,
        organization_slug: org.slug,
        organization_name: org.name,
        role: invitation.role,
        expires_at: invitation.expires_at,
    }))
}

pub async fn revoke_invitation(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
    Json(payload): Json<RevokeInvitationRequest>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    let invitation_repo = InvitationRepository::new(&state.pool);

    ensure_admin_access(&state.pool, org_id, user.id).await?;

    invitation_repo
        .revoke_invitation(org_id, payload.invitation_id, user.id)
        .await
        .map_err(|e| match e {
            IdentityError::PermissionDenied => {
                ErrorResponse::new(StatusCode::FORBIDDEN, "Admin access required")
            }
            IdentityError::NotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "Invitation not found")
            }
            _ => ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_invitation(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    let invitation_repo = InvitationRepository::new(&state.pool);

    let (org, role) = invitation_repo
        .accept_invitation(&token, user.id)
        .await
        .map_err(|e| match e {
            IdentityError::InvitationError(msg) => ErrorResponse::new(StatusCode::BAD_REQUEST, msg),
            IdentityError::NotFound => {
                ErrorResponse::new(StatusCode::NOT_FOUND, "Invitation not found")
            }
            _ => ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
        })?;

    Ok(Json(AcceptInvitationResponse {
        organization_id: org.id.to_string(),
        organization_slug: org.slug,
        role,
    }))
}

pub async fn list_members(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path(org_id): Path<Uuid>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    ensure_member_access(&state.pool, org_id, user.id).await?;

    let members = sqlx::query_as!(
        OrganizationMemberWithProfile,
        r#"
        SELECT
            omm.user_id AS "user_id!: Uuid",
            omm.role AS "role!: MemberRole",
            omm.joined_at AS "joined_at!",
            u.first_name AS "first_name?",
            u.last_name AS "last_name?",
            u.username AS "username?",
            u.email AS "email?",
            oa.avatar_url AS "avatar_url?"
        FROM organization_member_metadata omm
        INNER JOIN users u ON omm.user_id = u.id
        LEFT JOIN LATERAL (
            SELECT avatar_url
            FROM oauth_accounts
            WHERE user_id = omm.user_id
            ORDER BY created_at ASC
            LIMIT 1
        ) oa ON true
        WHERE omm.organization_id = $1
        ORDER BY omm.joined_at ASC
        "#,
        org_id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(ListMembersResponse { members }))
}

pub async fn remove_member(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path((org_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    if user.id == user_id {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Cannot remove yourself",
        ));
    }

    let org_repo = OrganizationRepository::new(&state.pool);
    if org_repo
        .is_personal(org_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, "Organization not found"))?
    {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Cannot modify members of a personal organization",
        ));
    }

    ensure_admin_access(&state.pool, org_id, user.id).await?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    let target = sqlx::query!(
        r#"
        SELECT role AS "role!: MemberRole"
        FROM organization_member_metadata
        WHERE organization_id = $1 AND user_id = $2
        FOR UPDATE
        "#,
        org_id,
        user_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
    .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "Member not found"))?;

    if target.role == MemberRole::Admin {
        let admin_ids = sqlx::query_scalar!(
            r#"
            SELECT user_id
            FROM organization_member_metadata
            WHERE organization_id = $1 AND role = 'admin'
            FOR UPDATE
            "#,
            org_id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

        if admin_ids.len() == 1 && admin_ids[0] == user_id {
            return Err(ErrorResponse::new(
                StatusCode::CONFLICT,
                "Cannot remove the last admin",
            ));
        }
    }

    sqlx::query!(
        r#"
        DELETE FROM organization_member_metadata
        WHERE organization_id = $1 AND user_id = $2
        "#,
        org_id,
        user_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    tx.commit()
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_member_role(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Path((org_id, user_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateMemberRoleRequest>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let user = ctx.user;
    if user.id == user_id && payload.role == MemberRole::Member {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Cannot demote yourself",
        ));
    }

    let org_repo = OrganizationRepository::new(&state.pool);
    if org_repo
        .is_personal(org_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, "Organization not found"))?
    {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Cannot modify members of a personal organization",
        ));
    }

    ensure_admin_access(&state.pool, org_id, user.id).await?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    let target = sqlx::query!(
        r#"
        SELECT role AS "role!: MemberRole"
        FROM organization_member_metadata
        WHERE organization_id = $1 AND user_id = $2
        FOR UPDATE
        "#,
        org_id,
        user_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
    .ok_or_else(|| ErrorResponse::new(StatusCode::NOT_FOUND, "Member not found"))?;

    if target.role == payload.role {
        return Ok(Json(UpdateMemberRoleResponse {
            user_id,
            role: payload.role,
        }));
    }

    if target.role == MemberRole::Admin && payload.role == MemberRole::Member {
        let admin_ids = sqlx::query_scalar!(
            r#"
            SELECT user_id
            FROM organization_member_metadata
            WHERE organization_id = $1 AND role = 'admin'
            FOR UPDATE
            "#,
            org_id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

        if admin_ids.len() == 1 && admin_ids[0] == user_id {
            return Err(ErrorResponse::new(
                StatusCode::CONFLICT,
                "Cannot demote the last admin",
            ));
        }
    }

    sqlx::query!(
        r#"
        UPDATE organization_member_metadata
        SET role = $3
        WHERE organization_id = $1 AND user_id = $2
        "#,
        org_id,
        user_id,
        payload.role as MemberRole
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    tx.commit()
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(UpdateMemberRoleResponse {
        user_id,
        role: payload.role,
    }))
}

pub(crate) async fn ensure_member_access(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<(), ErrorResponse> {
    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| membership_error(err, "Not a member of organization"))
}

pub(crate) async fn ensure_admin_access(
    pool: &PgPool,
    organization_id: Uuid,
    user_id: Uuid,
) -> Result<(), ErrorResponse> {
    OrganizationRepository::new(pool)
        .assert_admin(organization_id, user_id)
        .await
        .map_err(|err| membership_error(err, "Admin access required"))
}

pub(crate) async fn ensure_project_access(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<Uuid, ErrorResponse> {
    // Look up organization_id via node_projects table (legacy projects table removed)
    let organization_id = NodeProjectRepository::organization_id(pool, project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %project_id, "failed to load project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .ok_or_else(|| {
            warn!(
                %project_id,
                %user_id,
                "project not found for access check"
            );
            ErrorResponse::new(StatusCode::NOT_FOUND, "project not found")
        })?;

    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| {
            if let IdentityError::Database(error) = &err {
                tracing::error!(
                    ?error,
                    %organization_id,
                    %project_id,
                    "failed to authorize project membership"
                );
            } else {
                warn!(
                    ?err,
                    %organization_id,
                    %project_id,
                    %user_id,
                    "project access denied"
                );
            }
            membership_error(err, "project not accessible")
        })?;

    Ok(organization_id)
}

pub(crate) async fn ensure_task_access(
    pool: &PgPool,
    user_id: Uuid,
    task_id: Uuid,
) -> Result<Uuid, ErrorResponse> {
    let organization_id = SharedTaskRepository::organization_id(pool, task_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %task_id, "failed to load shared task");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .ok_or_else(|| {
            warn!(
                %task_id,
                %user_id,
                "shared task not found for access check"
            );
            ErrorResponse::new(StatusCode::NOT_FOUND, "shared task not found")
        })?;

    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| {
            if let IdentityError::Database(error) = &err {
                tracing::error!(
                    ?error,
                    %organization_id,
                    %task_id,
                    "failed to authorize shared task access"
                );
            } else {
                warn!(
                    ?err,
                    %organization_id,
                    %task_id,
                    %user_id,
                    "shared task access denied"
                );
            }
            membership_error(err, "task not accessible")
        })?;

    Ok(organization_id)
}

pub(crate) async fn ensure_swarm_project_access(
    pool: &PgPool,
    user_id: Uuid,
    swarm_project_id: Uuid,
) -> Result<Uuid, ErrorResponse> {
    // Lookup organization_id from swarm_projects table
    let organization_id = SwarmProjectRepository::organization_id(pool, swarm_project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, %swarm_project_id, "failed to load swarm project");
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?
        .ok_or_else(|| {
            tracing::warn!(%swarm_project_id, %user_id, "swarm project not found for access check");
            ErrorResponse::new(StatusCode::NOT_FOUND, "project not found")
        })?;

    // Verify user is a member of the organization
    organization_members::assert_membership(pool, organization_id, user_id)
        .await
        .map_err(|err| membership_error(err, "project not accessible"))?;

    Ok(organization_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_swarm_project_access_signature() {
        // This test verifies that the function exists and has the correct signature.
        // Integration tests would require a database and proper test fixtures.
        // The function signature is:
        // pub(crate) async fn ensure_swarm_project_access(
        //     pool: &PgPool,
        //     user_id: Uuid,
        //     swarm_project_id: Uuid,
        // ) -> Result<Uuid, ErrorResponse>
        // This test simply ensures the function is accessible within the module.
        let _fn = ensure_swarm_project_access;
        // If this compiles, the signature is correct.
    }

    #[test]
    fn test_error_response_creation() {
        // Test that ErrorResponse can be created with valid status codes
        // These are used by ensure_swarm_project_access for error handling
        let _not_found = ErrorResponse::new(StatusCode::NOT_FOUND, "project not found");
        let _forbidden = ErrorResponse::new(StatusCode::FORBIDDEN, "project not accessible");
        let _internal_error =
            ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error");
        // If creation succeeds, error handling is available
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use chrono::Utc;
    use sqlx::PgPool;

    /// Helper to get database URL from environment.
    fn database_url() -> Option<String> {
        std::env::var("SERVER_DATABASE_URL")
            .ok()
            .or_else(|| std::env::var("DATABASE_URL").ok())
    }

    /// Skip test if database is not available.
    macro_rules! skip_without_db {
        () => {
            if database_url().is_none() {
                eprintln!("Skipping test: DATABASE_URL or SERVER_DATABASE_URL not set");
                return;
            }
        };
    }

    /// Create a test database connection pool.
    async fn create_pool() -> PgPool {
        let url = database_url().expect("DATABASE_URL must be set");
        PgPool::connect(&url)
            .await
            .expect("Failed to connect to database")
    }

    /// Helper to create test organization
    async fn create_test_organization(pool: &PgPool) -> Uuid {
        let org_id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO organizations (id, name, slug, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(org_id)
        .bind(format!("Test Org {}", org_id))
        .bind(format!(
            "test-org-{}",
            org_id.to_string()[..8].to_lowercase()
        ))
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create test organization");

        org_id
    }

    /// Helper to create test user
    async fn create_test_user(pool: &PgPool) -> Uuid {
        let user_id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO users (id, email, username, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(user_id)
        .bind(format!(
            "test-{}@example.com",
            user_id.to_string()[..8].to_lowercase()
        ))
        .bind(format!("test_{}", user_id.to_string()[..8].to_lowercase()))
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create test user");

        user_id
    }

    /// Helper to add user to organization
    async fn add_user_to_org(pool: &PgPool, user_id: Uuid, org_id: Uuid) {
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO organization_member_metadata (organization_id, user_id, role, joined_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .bind("member")
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to add user to organization");
    }

    /// Helper to create test swarm project
    async fn create_test_swarm_project(pool: &PgPool, org_id: Uuid) -> Uuid {
        let swarm_project_id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO swarm_projects (id, organization_id, name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(swarm_project_id)
        .bind(org_id)
        .bind(format!("Test Swarm Project {}", swarm_project_id))
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create test swarm project");

        swarm_project_id
    }

    /// Cleanup helper - remove test organization and its members
    async fn cleanup_org(pool: &PgPool, org_id: Uuid) {
        let _ = sqlx::query(
            r#"
            DELETE FROM organization_member_metadata
            WHERE organization_id = $1
            "#,
        )
        .bind(org_id)
        .execute(pool)
        .await;

        let _ = sqlx::query(
            r#"
            DELETE FROM swarm_projects
            WHERE organization_id = $1
            "#,
        )
        .bind(org_id)
        .execute(pool)
        .await;

        let _ = sqlx::query(
            r#"
            DELETE FROM organizations
            WHERE id = $1
            "#,
        )
        .bind(org_id)
        .execute(pool)
        .await;
    }

    /// Cleanup helper - remove test user
    async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
        let _ = sqlx::query(
            r#"
            DELETE FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .execute(pool)
        .await;
    }

    /// Test: ensure_swarm_project_access succeeds when user has membership
    #[tokio::test]
    async fn test_ensure_swarm_project_access_success() {
        skip_without_db!();

        let pool = create_pool().await;
        let org_id = create_test_organization(&pool).await;
        let user_id = create_test_user(&pool).await;
        add_user_to_org(&pool, user_id, org_id).await;
        let swarm_project_id = create_test_swarm_project(&pool, org_id).await;

        let result = ensure_swarm_project_access(&pool, user_id, swarm_project_id).await;

        assert!(
            result.is_ok(),
            "ensure_swarm_project_access should succeed for organization member"
        );
        assert_eq!(
            result.unwrap(),
            org_id,
            "Should return the correct organization_id"
        );

        // Cleanup
        cleanup_org(&pool, org_id).await;
        cleanup_user(&pool, user_id).await;
    }

    /// Test: ensure_swarm_project_access returns NOT_FOUND for non-existent swarm project
    #[tokio::test]
    async fn test_ensure_swarm_project_access_not_found() {
        skip_without_db!();

        let pool = create_pool().await;
        let user_id = create_test_user(&pool).await;
        let nonexistent_swarm_project_id = Uuid::new_v4();

        let result =
            ensure_swarm_project_access(&pool, user_id, nonexistent_swarm_project_id).await;

        assert!(
            result.is_err(),
            "ensure_swarm_project_access should fail for non-existent swarm project"
        );
        let error = result.unwrap_err();
        assert_eq!(
            error.status(),
            StatusCode::NOT_FOUND,
            "Should return 404 Not Found for non-existent swarm project"
        );

        // Cleanup
        cleanup_user(&pool, user_id).await;
    }

    /// Test: ensure_swarm_project_access returns FORBIDDEN when user lacks organization membership
    #[tokio::test]
    async fn test_ensure_swarm_project_access_forbidden() {
        skip_without_db!();

        let pool = create_pool().await;
        let org_id = create_test_organization(&pool).await;
        let user_id = create_test_user(&pool).await;
        // Note: user is NOT added to organization
        let swarm_project_id = create_test_swarm_project(&pool, org_id).await;

        let result = ensure_swarm_project_access(&pool, user_id, swarm_project_id).await;

        assert!(
            result.is_err(),
            "ensure_swarm_project_access should fail for user without organization membership"
        );
        let error = result.unwrap_err();
        assert_eq!(
            error.status(),
            StatusCode::FORBIDDEN,
            "Should return 403 Forbidden for user lacking organization membership"
        );

        // Cleanup
        cleanup_org(&pool, org_id).await;
        cleanup_user(&pool, user_id).await;
    }
}
