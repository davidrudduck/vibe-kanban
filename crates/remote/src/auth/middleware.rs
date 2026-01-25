use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::headers::{Authorization, HeaderMapExt, authorization::Bearer};
use chrono::{DateTime, Utc};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    AppState,
    db::{
        auth::{AuthSessionError, AuthSessionRepository, MAX_SESSION_INACTIVITY_DURATION},
        identity_errors::IdentityError,
        users::{User, UserRepository},
    },
    nodes::NodeServiceImpl,
};

/// Context for user-authenticated requests (via OAuth JWT).
#[derive(Clone)]
pub struct RequestContext {
    pub user: User,
    pub session_id: Uuid,
    pub access_token_expires_at: DateTime<Utc>,
}

/// Context for node-authenticated requests (via API key).
///
/// Used when a node makes REST API calls using its API key instead of
/// user OAuth tokens. This allows nodes to sync without requiring user login.
#[derive(Clone)]
#[allow(dead_code)] // Fields reserved for future authorization checks
pub struct NodeAuthContext {
    /// The organization ID from the validated API key
    pub organization_id: Uuid,
    /// The node ID bound to this API key (if any)
    pub node_id: Option<Uuid>,
    /// The API key ID used for authentication
    pub api_key_id: Uuid,
}

/// Combined context that can be either user or node authentication.
///
/// Routes that accept either authentication method should extract this
/// to handle both cases.
#[derive(Clone)]
pub enum AuthContext {
    /// User authenticated via OAuth JWT
    User(RequestContext),
    /// Node authenticated via API key
    Node(NodeAuthContext),
}

impl AuthContext {
    /// Returns the organization ID for this authenticated request.
    ///
    /// For node auth, this is the API key's organization.
    /// For user auth, this must be determined from the request parameters.
    #[allow(dead_code)] // Reserved for future use
    /// Retrieve the organization ID when the authentication context represents a node.
    ///
    /// # Returns
    ///
    /// `Some(Uuid)` containing the organization ID for `AuthContext::Node`, `None` for `AuthContext::User`.
    ///
    /// # Examples
    ///
    /// ```
    /// use uuid::Uuid;
    ///
    /// let org = Uuid::new_v4();
    /// let node_ctx = NodeAuthContext { organization_id: org, node_id: None, api_key_id: Uuid::new_v4() };
    /// let ctx = AuthContext::Node(node_ctx);
    /// assert_eq!(ctx.node_organization_id(), Some(org));
    ///
    /// let user_ctx = AuthContext::User(RequestContext { /* fields omitted for brevity */ user: todo!(), session_id: Uuid::new_v4(), access_token_expires_at: chrono::Utc::now() });
    /// assert_eq!(user_ctx.node_organization_id(), None);
    /// ```
    pub fn node_organization_id(&self) -> Option<Uuid> {
        match self {
            AuthContext::Node(ctx) => Some(ctx.organization_id),
            AuthContext::User(_) => None,
        }
    }
}

/// Enforces a valid user session from an OAuth bearer token and inserts a `RequestContext` into the request extensions.
///
/// On success, forwards the request to the next handler and returns that handler's `Response`.
/// On failure, returns an HTTP `401 Unauthorized` for invalid or expired sessions (including revoked or missing session/user)
/// or `500 Internal Server Error` for database errors encountered while loading session or user.
///
/// # Examples
///
/// ```no_run
/// use axum::{Router, routing::get};
/// use crate::auth::middleware::require_session;
///
/// // Mount the middleware on a route so handlers receive an authenticated RequestContext.
/// let app = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(axum::middleware::from_fn(require_session));
/// ```
pub async fn require_session(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let bearer = match req.headers().typed_get::<Authorization<Bearer>>() {
        Some(Authorization(token)) => token.token().to_owned(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let jwt = state.jwt();
    let identity = match jwt.decode_access_token(&bearer) {
        Ok(details) => details,
        Err(error) => {
            warn!(?error, "failed to decode access token");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let pool = state.pool();
    let session_repo = AuthSessionRepository::new(pool);
    let session = match session_repo.get(identity.session_id).await {
        Ok(session) => session,
        Err(AuthSessionError::NotFound) => {
            warn!("session `{}` not found", identity.session_id);
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Err(AuthSessionError::Database(error)) => {
            warn!(?error, "failed to load session");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Err(_) => {
            warn!("failed to load session for unknown reason");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    if session.revoked_at.is_some() {
        warn!("session `{}` rejected (revoked)", identity.session_id);
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if session.inactivity_duration(Utc::now()) > MAX_SESSION_INACTIVITY_DURATION {
        warn!(
            "session `{}` expired due to inactivity; revoking",
            identity.session_id
        );
        if let Err(error) = session_repo.revoke(session.id).await {
            warn!(?error, "failed to revoke inactive session");
        }
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let user_repo = UserRepository::new(pool);
    let user = match user_repo.fetch_user(identity.user_id).await {
        Ok(user) => user,
        Err(IdentityError::NotFound) => {
            warn!("user `{}` missing", identity.user_id);
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Err(IdentityError::Database(error)) => {
            warn!(?error, "failed to load user");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Err(_) => {
            warn!("unexpected error loading user");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    req.extensions_mut().insert(RequestContext {
        user,
        session_id: session.id,
        access_token_expires_at: identity.expires_at,
    });

    match session_repo.touch(session.id).await {
        Ok(_) => {}
        Err(error) => warn!(?error, "failed to update session last-used timestamp"),
    }

    next.run(req).await
}

/// Authenticate the incoming request using either a user OAuth session or a node API key and inject the appropriate auth context into request extensions.
///
/// When a valid user session is presented, inserts a `RequestContext` (for legacy compatibility) and `AuthContext::User`. If user session validation fails or is not applicable, attempts API-key authentication and, on success, inserts `AuthContext::Node`.
///
/// Returns an HTTP response produced by the next middleware/handler on success, or an appropriate `401`/`500` response for authentication or internal errors.
///
/// # Examples
///
/// ```no_run
/// // Typical usage is as a tower/http middleware handler. This sketch demonstrates the call shape.
/// # use http::Request;
/// # use hyper::Body;
/// # use tower::ServiceExt;
/// # async fn example_call(state: crate::AppState, req: Request<Body>, next: crate::Next) {
/// let _resp = crate::auth::require_session_or_node_api_key(state.into(), req, next).await;
/// # }
/// ```
pub async fn require_session_or_node_api_key(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let bearer = match req.headers().typed_get::<Authorization<Bearer>>() {
        Some(Authorization(token)) => token.token().to_owned(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // Try OAuth JWT first
    let jwt = state.jwt();
    if let Ok(identity) = jwt.decode_access_token(&bearer) {
        // Valid JWT - continue with user session validation
        let pool = state.pool();
        let session_repo = AuthSessionRepository::new(pool);
        let session = match session_repo.get(identity.session_id).await {
            Ok(session) => session,
            Err(AuthSessionError::NotFound) => {
                debug!("session `{}` not found, will try API key", identity.session_id);
                // Fall through to API key validation
                return try_api_key_auth(state, req, &bearer, next).await;
            }
            Err(AuthSessionError::Database(error)) => {
                warn!(?error, "failed to load session");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Err(_) => {
                debug!("failed to load session, will try API key");
                return try_api_key_auth(state, req, &bearer, next).await;
            }
        };

        if session.revoked_at.is_some() {
            debug!("session `{}` revoked, will try API key", identity.session_id);
            return try_api_key_auth(state, req, &bearer, next).await;
        }

        if session.inactivity_duration(Utc::now()) > MAX_SESSION_INACTIVITY_DURATION {
            debug!("session `{}` expired, will try API key", identity.session_id);
            if let Err(error) = session_repo.revoke(session.id).await {
                warn!(?error, "failed to revoke inactive session");
            }
            return try_api_key_auth(state, req, &bearer, next).await;
        }

        let user_repo = UserRepository::new(pool);
        let user = match user_repo.fetch_user(identity.user_id).await {
            Ok(user) => user,
            Err(IdentityError::NotFound) => {
                debug!("user `{}` missing, will try API key", identity.user_id);
                return try_api_key_auth(state, req, &bearer, next).await;
            }
            Err(IdentityError::Database(error)) => {
                warn!(?error, "failed to load user");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Err(_) => {
                warn!("unexpected error loading user");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        let user_ctx = RequestContext {
            user,
            session_id: session.id,
            access_token_expires_at: identity.expires_at,
        };

        // Insert both the legacy RequestContext and the new AuthContext
        req.extensions_mut().insert(user_ctx.clone());
        req.extensions_mut()
            .insert(AuthContext::User(user_ctx));

        // Touch the session
        match session_repo.touch(session.id).await {
            Ok(_) => {}
            Err(error) => warn!(?error, "failed to update session last-used timestamp"),
        }

        return next.run(req).await;
    }

    // JWT decode failed - try API key auth
    try_api_key_auth(state, req, &bearer, next).await
}

/// Authenticate a request using a raw API key and attach node authentication context on success.
///
/// On successful validation, inserts `AuthContext::Node` and a `NodeAuthContext` into the request
/// extensions and forwards the request to the next handler. If validation fails, returns a
/// `401 Unauthorized` response.
///
/// # Examples
///
/// ```
/// # use axum::http::Request;
/// # use axum::body::Body;
/// # use axum::response::Response;
/// # use axum::middleware::Next;
/// # use uuid::Uuid;
/// # async fn example_call(
/// #     state: crate::AppState,
/// #     req: Request<Body>,
/// #     next: Next,
/// # ) -> Response {
/// let raw_key = "api_key_string";
/// // try_api_key_auth returns a response that either continues the chain or is 401
/// let resp = crate::auth::middleware::try_api_key_auth(state, req, raw_key, next).await;
/// resp
/// # }
/// ```
async fn try_api_key_auth(
    state: AppState,
    mut req: Request<Body>,
    raw_key: &str,
    next: Next,
) -> Response {
    let pool = state.pool();
    let service = NodeServiceImpl::new(pool.clone());

    match service.validate_api_key(raw_key).await {
        Ok(api_key) => {
            debug!(
                api_key_id = %api_key.id,
                organization_id = %api_key.organization_id,
                "API key authentication successful"
            );

            let node_ctx = NodeAuthContext {
                organization_id: api_key.organization_id,
                node_id: api_key.node_id,
                api_key_id: api_key.id,
            };

            req.extensions_mut()
                .insert(AuthContext::Node(node_ctx.clone()));
            req.extensions_mut().insert(node_ctx);

            next.run(req).await
        }
        Err(e) => {
            debug!(?e, "API key validation failed");
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
}
