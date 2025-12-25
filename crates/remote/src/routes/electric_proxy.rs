//! Electric Shape API proxy routes.
//!
//! These routes proxy requests to the Electric sync service with authentication
//! and organization-based WHERE clause injection for security.

use std::collections::HashMap;

use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures::TryStreamExt;
use secrecy::ExposeSecret;
use tracing::error;
use uuid::Uuid;

use crate::{
    AppState, auth::RequestContext, db::organizations::OrganizationRepository,
    validated_where::ValidatedWhere,
};

/// Creates the Electric proxy router.
pub fn router() -> Router<AppState> {
    Router::new().route("/shape/shared_tasks", get(proxy_shared_tasks))
}

/// Electric protocol query parameters that are safe to forward.
/// Based on https://electric-sql.com/docs/guides/auth#proxy-auth
/// Note: "where" is NOT included because it's controlled server-side for security.
const ELECTRIC_PARAMS: &[&str] = &["offset", "handle", "live", "cursor", "columns"];

/// Returns an empty shape response for users with no organization memberships.
fn empty_shape_response() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    (StatusCode::OK, headers, "[]").into_response()
}

/// Proxy Shape requests for the `shared_tasks` table.
///
/// Route: GET /v1/shape/shared_tasks?offset=-1
///
/// The `require_session` middleware has already validated the Bearer token
/// before this handler is called.
pub async fn proxy_shared_tasks(
    State(state): State<AppState>,
    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ProxyError> {
    // Check if Electric is configured
    let electric_url = state
        .config
        .electric_url
        .as_ref()
        .ok_or_else(|| ProxyError::NotConfigured)?;

    // Get user's organization memberships
    let org_repo = OrganizationRepository::new(state.pool());
    let orgs = org_repo
        .list_user_organizations(ctx.user.id)
        .await
        .map_err(|e| ProxyError::Authorization(format!("failed to fetch organizations: {e}")))?;

    if orgs.is_empty() {
        // User has no org memberships - return empty result
        return Ok(empty_shape_response());
    }

    // Build org_id filter using server-side WHERE clause (security: not from client)
    let org_uuids: Vec<Uuid> = orgs.iter().map(|o| o.id).collect();
    let query = ValidatedWhere::new("shared_tasks", r#""organization_id" = ANY($1)"#);
    let query_params = &[format!(
        "{{{}}}",
        org_uuids
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )];
    tracing::debug!("Proxying Electric Shape request for shared_tasks table: {:?}", query);
    proxy_table(&state, electric_url, &query, &params, query_params).await
}

/// Proxy a Shape request to Electric for a specific table.
///
/// The table and where clause are set server-side (not from client params)
/// to prevent unauthorized access to other tables or data.
async fn proxy_table(
    state: &AppState,
    electric_url: &str,
    query: &ValidatedWhere,
    client_params: &HashMap<String, String>,
    electric_params: &[String],
) -> Result<Response, ProxyError> {
    // Build the Electric URL
    let mut origin_url = url::Url::parse(electric_url)
        .map_err(|e| ProxyError::InvalidConfig(format!("invalid electric_url: {e}")))?;

    origin_url.set_path("/v1/shape");

    // Set table server-side (security: client can't override)
    origin_url
        .query_pairs_mut()
        .append_pair("table", query.table);

    // Set WHERE clause with parameterized values
    origin_url
        .query_pairs_mut()
        .append_pair("where", query.where_clause);

    // Pass params for $1, $2, etc. placeholders
    for (i, param) in electric_params.iter().enumerate() {
        origin_url
            .query_pairs_mut()
            .append_pair(&format!("params[{}]", i + 1), param);
    }

    // Forward safe client params
    for (key, value) in client_params {
        if ELECTRIC_PARAMS.contains(&key.as_str()) {
            origin_url.query_pairs_mut().append_pair(key, value);
        }
    }

    // Add Electric secret if configured
    if let Some(secret) = &state.config.electric_secret {
        origin_url
            .query_pairs_mut()
            .append_pair("secret", secret.expose_secret());
    }

    let response = state
        .http_client
        .get(origin_url.as_str())
        .send()
        .await
        .map_err(ProxyError::Connection)?;

    let status = response.status();

    let mut headers = HeaderMap::new();

    // Copy headers from Electric response, but remove problematic ones
    for (key, value) in response.headers() {
        // Skip headers that interfere with browser handling
        if key == header::CONTENT_ENCODING || key == header::CONTENT_LENGTH {
            continue;
        }
        headers.insert(key.clone(), value.clone());
    }

    // Add Vary header for proper caching with auth
    headers.insert(header::VARY, HeaderValue::from_static("Authorization"));

    // Stream the response body directly without buffering
    let body_stream = response.bytes_stream().map_err(std::io::Error::other);
    let body = Body::from_stream(body_stream);

    Ok((status, headers, body).into_response())
}

#[derive(Debug)]
pub enum ProxyError {
    /// Electric is not configured
    NotConfigured,
    /// Failed to connect to Electric service
    Connection(reqwest::Error),
    /// Invalid Electric configuration
    InvalidConfig(String),
    /// Authorization failed
    Authorization(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        match self {
            ProxyError::NotConfigured => {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Electric sync service is not configured",
                )
                    .into_response()
            }
            ProxyError::Connection(err) => {
                error!(?err, "failed to connect to Electric service");
                (
                    StatusCode::BAD_GATEWAY,
                    "failed to connect to Electric service",
                )
                    .into_response()
            }
            ProxyError::InvalidConfig(msg) => {
                error!(%msg, "invalid Electric proxy configuration");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
            }
            ProxyError::Authorization(msg) => {
                error!(%msg, "authorization failed for Electric proxy");
                (StatusCode::FORBIDDEN, "forbidden").into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_electric_params_list() {
        // Verify that "where" is NOT in the list (security)
        assert!(!ELECTRIC_PARAMS.contains(&"where"));
        assert!(!ELECTRIC_PARAMS.contains(&"table"));

        // Verify safe params are included
        assert!(ELECTRIC_PARAMS.contains(&"offset"));
        assert!(ELECTRIC_PARAMS.contains(&"handle"));
        assert!(ELECTRIC_PARAMS.contains(&"live"));
        assert!(ELECTRIC_PARAMS.contains(&"cursor"));
        assert!(ELECTRIC_PARAMS.contains(&"columns"));
    }

    #[test]
    fn test_proxy_error_display() {
        let err = ProxyError::NotConfigured;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let err = ProxyError::InvalidConfig("test".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let err = ProxyError::Authorization("test".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_empty_shape_response() {
        let response = empty_shape_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_format_org_uuids_for_electric() {
        // Test the format used for ANY($1) array parameter
        let org_uuids = [
            Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
        ];

        let formatted = format!(
            "{{{}}}",
            org_uuids
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        assert_eq!(
            formatted,
            "{11111111-1111-1111-1111-111111111111,22222222-2222-2222-2222-222222222222}"
        );
    }

    #[test]
    fn test_url_building() {
        // Test that URL building works correctly
        let mut url = url::Url::parse("http://localhost:3001").unwrap();
        url.set_path("/v1/shape");
        url.query_pairs_mut()
            .append_pair("table", "shared_tasks")
            .append_pair("where", r#""organization_id" = ANY($1)"#)
            .append_pair("params[1]", "{uuid1,uuid2}")
            .append_pair("offset", "-1")
            .append_pair("live", "true");

        let url_str = url.as_str();
        assert!(url_str.contains("table=shared_tasks"));
        assert!(url_str.contains("offset=-1"));
        assert!(url_str.contains("live=true"));
        assert!(url_str.contains("params%5B1%5D")); // params[1] URL-encoded
    }

    #[test]
    fn test_filter_unsafe_params() {
        // Test that unsafe params are filtered out
        let client_params: HashMap<String, String> = [
            ("offset".to_string(), "-1".to_string()),
            ("where".to_string(), "malicious".to_string()), // Should be filtered
            ("table".to_string(), "other_table".to_string()), // Should be filtered
            ("live".to_string(), "true".to_string()),
        ]
        .into_iter()
        .collect();

        let safe_params: Vec<&String> = client_params
            .iter()
            .filter(|(k, _)| ELECTRIC_PARAMS.contains(&k.as_str()))
            .map(|(_, v)| v)
            .collect();

        assert_eq!(safe_params.len(), 2);
        assert!(safe_params.contains(&&"-1".to_string()));
        assert!(safe_params.contains(&&"true".to_string()));
    }
}
