//! Electric Shape API client for syncing data from Postgres via ElectricSQL.
//!
//! This module provides a client for consuming Electric Shape API responses,
//! parsing shape operations (insert, update, delete) and control messages
//! (up-to-date, must-refetch).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Error types for Electric Shape API operations.
#[derive(Debug, Error)]
pub enum ElectricError {
    /// Failed to parse JSON from Electric API response.
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Unknown operation type in shape message.
    #[error("unknown operation type: {0}")]
    UnknownOperation(String),

    /// Unknown control message type.
    #[error("unknown control message: {0}")]
    UnknownControl(String),

    /// Missing required field in message.
    #[error("missing required field: {0}")]
    MissingField(String),

    /// HTTP transport error.
    #[error("transport error: {0}")]
    Transport(String),

    /// HTTP status error.
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
}

/// Represents the type of operation in a shape message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    Insert,
    Update,
    Delete,
}

/// Represents a control message from the Electric API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlType {
    UpToDate,
    MustRefetch,
}

/// Headers from an Electric Shape message.
#[derive(Debug, Clone, Deserialize)]
pub struct ShapeHeaders {
    /// Operation type (insert, update, delete) - present for data operations.
    #[serde(default)]
    pub operation: Option<OperationType>,

    /// Control message type (up-to-date, must-refetch) - present for control messages.
    #[serde(default)]
    pub control: Option<ControlType>,
}

/// A raw message from the Electric Shape API.
#[derive(Debug, Clone, Deserialize)]
pub struct RawShapeMessage {
    /// Headers containing operation or control information.
    pub headers: ShapeHeaders,

    /// The primary key of the row (present for data operations).
    #[serde(default)]
    pub key: Option<String>,

    /// The row data (present for insert and update operations).
    #[serde(default)]
    pub value: Option<Value>,
}

/// A parsed shape operation from the Electric API.
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeOperation {
    /// An insert operation with key and value.
    Insert { key: String, value: Value },

    /// An update operation with key and value.
    Update { key: String, value: Value },

    /// A delete operation with just the key.
    Delete { key: String },

    /// Indicates the client is up-to-date with the server.
    UpToDate,

    /// Indicates the client must refetch the entire shape.
    MustRefetch,
}

impl ShapeOperation {
    /// Parse a JSON string into a ShapeOperation.
    pub fn parse(json: &str) -> Result<Self, ElectricError> {
        let raw: RawShapeMessage = serde_json::from_str(json)?;
        Self::from_raw(raw)
    }

    /// Convert a RawShapeMessage into a ShapeOperation.
    pub fn from_raw(raw: RawShapeMessage) -> Result<Self, ElectricError> {
        // Check for control messages first
        if let Some(control) = raw.headers.control {
            return match control {
                ControlType::UpToDate => Ok(ShapeOperation::UpToDate),
                ControlType::MustRefetch => Ok(ShapeOperation::MustRefetch),
            };
        }

        // Handle data operations
        if let Some(operation) = raw.headers.operation {
            let key = raw
                .key
                .ok_or_else(|| ElectricError::MissingField("key".to_string()))?;

            match operation {
                OperationType::Insert => {
                    let value = raw
                        .value
                        .ok_or_else(|| ElectricError::MissingField("value".to_string()))?;
                    Ok(ShapeOperation::Insert { key, value })
                }
                OperationType::Update => {
                    let value = raw
                        .value
                        .ok_or_else(|| ElectricError::MissingField("value".to_string()))?;
                    Ok(ShapeOperation::Update { key, value })
                }
                OperationType::Delete => Ok(ShapeOperation::Delete { key }),
            }
        } else {
            Err(ElectricError::MissingField(
                "operation or control".to_string(),
            ))
        }
    }
}

/// Configuration for an Electric shape subscription.
#[derive(Debug, Clone)]
pub struct ShapeConfig {
    /// Base URL for the Electric API (e.g., "http://localhost:3000").
    pub base_url: String,

    /// Table name to subscribe to.
    pub table: String,

    /// Optional WHERE clause for filtering.
    pub where_clause: Option<String>,

    /// Optional columns to select.
    pub columns: Option<Vec<String>>,
}

/// Represents the state of a shape subscription.
#[derive(Debug, Clone, Default)]
pub struct ShapeState {
    /// The shape handle returned by Electric.
    pub handle: Option<String>,

    /// The current offset in the shape log.
    pub offset: String,
}

impl ShapeState {
    /// Create a new ShapeState for an initial sync.
    pub fn initial() -> Self {
        Self {
            handle: None,
            offset: "-1".to_string(),
        }
    }
}

/// HTTP response headers from the Electric API.
#[derive(Debug, Clone, Default)]
pub struct ElectricHeaders {
    /// Shape handle for resuming sync.
    pub handle: Option<String>,

    /// Current sync offset.
    pub offset: Option<String>,

    /// Schema information (JSON string).
    pub schema: Option<String>,
}

/// HTTP client for the Electric Shape API.
#[derive(Debug, Clone)]
pub struct ElectricClient {
    http: reqwest::Client,
    config: ShapeConfig,
}

impl ElectricClient {
    /// Create a new ElectricClient with the given configuration.
    pub fn new(config: ShapeConfig) -> Result<Self, ElectricError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("electric-sync/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ElectricError::Transport(e.to_string()))?;

        Ok(Self { http, config })
    }

    /// Build the URL for a shape request.
    pub fn build_url(&self, state: &ShapeState, live: bool) -> String {
        let mut url = format!(
            "{}/v1/shape?table={}&offset={}",
            self.config.base_url, self.config.table, state.offset
        );

        if let Some(ref handle) = state.handle {
            url.push_str(&format!("&handle={}", handle));
        }

        if let Some(ref where_clause) = self.config.where_clause {
            url.push_str(&format!("&where={}", urlencoding::encode(where_clause)));
        }

        if let Some(ref columns) = self.config.columns {
            url.push_str(&format!("&columns={}", columns.join(",")));
        }

        if live {
            url.push_str("&live=true");
        }

        url
    }

    /// Fetch the next batch of shape operations.
    ///
    /// Returns a tuple of (operations, new_state).
    pub async fn fetch(
        &self,
        state: &ShapeState,
        live: bool,
    ) -> Result<(Vec<ShapeOperation>, ShapeState), ElectricError> {
        let url = self.build_url(state, live);

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ElectricError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ElectricError::Http {
                status: status.as_u16(),
                body,
            });
        }

        // Extract headers
        let headers = Self::extract_headers(&response);

        // Parse NDJSON body
        let body = response
            .text()
            .await
            .map_err(|e| ElectricError::Transport(e.to_string()))?;

        let operations = Self::parse_ndjson(&body)?;

        // Update state
        let new_state = ShapeState {
            handle: headers.handle.or_else(|| state.handle.clone()),
            offset: headers.offset.unwrap_or_else(|| state.offset.clone()),
        };

        Ok((operations, new_state))
    }

    /// Extract Electric headers from response.
    fn extract_headers(response: &reqwest::Response) -> ElectricHeaders {
        ElectricHeaders {
            handle: response
                .headers()
                .get("electric-handle")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            offset: response
                .headers()
                .get("electric-offset")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            schema: response
                .headers()
                .get("electric-schema")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
        }
    }

    /// Parse newline-delimited JSON (NDJSON) into shape operations.
    pub fn parse_ndjson(body: &str) -> Result<Vec<ShapeOperation>, ElectricError> {
        let mut operations = Vec::new();

        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            operations.push(ShapeOperation::parse(trimmed)?);
        }

        Ok(operations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================
    // ShapeOperation Tests
    // ==================

    #[test]
    fn test_parse_shape_insert() {
        let json = r#"{"key":"task-1","value":{"id":"task-1","title":"Test"},"headers":{"operation":"insert"}}"#;
        let op = ShapeOperation::parse(json).unwrap();
        assert!(
            matches!(op, ShapeOperation::Insert { key, value } if key == "task-1" && value["title"] == "Test")
        );
    }

    #[test]
    fn test_parse_shape_update() {
        let json = r#"{"key":"task-1","value":{"id":"task-1","title":"Updated"},"headers":{"operation":"update"}}"#;
        let op = ShapeOperation::parse(json).unwrap();
        assert!(
            matches!(op, ShapeOperation::Update { key, value } if key == "task-1" && value["title"] == "Updated")
        );
    }

    #[test]
    fn test_parse_shape_delete() {
        let json = r#"{"key":"task-1","headers":{"operation":"delete"}}"#;
        let op = ShapeOperation::parse(json).unwrap();
        assert!(matches!(op, ShapeOperation::Delete { key } if key == "task-1"));
    }

    #[test]
    fn test_parse_control_up_to_date() {
        let json = r#"{"headers":{"control":"up-to-date"}}"#;
        let op = ShapeOperation::parse(json).unwrap();
        assert!(matches!(op, ShapeOperation::UpToDate));
    }

    #[test]
    fn test_parse_control_must_refetch() {
        let json = r#"{"headers":{"control":"must-refetch"}}"#;
        let op = ShapeOperation::parse(json).unwrap();
        assert!(matches!(op, ShapeOperation::MustRefetch));
    }

    #[test]
    fn test_parse_insert_missing_value() {
        let json = r#"{"key":"task-1","headers":{"operation":"insert"}}"#;
        let result = ShapeOperation::parse(json);
        assert!(matches!(result, Err(ElectricError::MissingField(f)) if f == "value"));
    }

    #[test]
    fn test_parse_missing_operation_and_control() {
        let json = r#"{"key":"task-1","headers":{}}"#;
        let result = ShapeOperation::parse(json);
        assert!(matches!(
            result,
            Err(ElectricError::MissingField(f)) if f == "operation or control"
        ));
    }

    #[test]
    fn test_parse_delete_missing_key() {
        let json = r#"{"headers":{"operation":"delete"}}"#;
        let result = ShapeOperation::parse(json);
        assert!(matches!(result, Err(ElectricError::MissingField(f)) if f == "key"));
    }

    // ==================
    // RawShapeMessage Tests
    // ==================

    #[test]
    fn test_raw_message_with_complex_value() {
        let json = r#"{
            "key": "\"public\".\"tasks\"/\"123e4567-e89b-12d3-a456-426614174000\"",
            "value": {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "title": "Test Task",
                "status": "todo",
                "project_id": "987e6543-e21b-12d3-a456-426614174000",
                "created_at": "2025-01-01T00:00:00Z"
            },
            "headers": {"operation": "insert"}
        }"#;

        let op = ShapeOperation::parse(json).unwrap();
        match op {
            ShapeOperation::Insert { key, value } => {
                assert!(key.contains("tasks"));
                assert_eq!(value["title"], "Test Task");
                assert_eq!(value["status"], "todo");
            }
            _ => panic!("Expected Insert operation"),
        }
    }

    // ==================
    // NDJSON Parsing Tests
    // ==================

    #[test]
    fn test_parse_ndjson_multiple_operations() {
        let ndjson = r#"{"key":"t-1","value":{"id":"t-1","title":"First"},"headers":{"operation":"insert"}}
{"key":"t-2","value":{"id":"t-2","title":"Second"},"headers":{"operation":"insert"}}
{"headers":{"control":"up-to-date"}}"#;

        let ops = ElectricClient::parse_ndjson(ndjson).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], ShapeOperation::Insert { key, .. } if key == "t-1"));
        assert!(matches!(&ops[1], ShapeOperation::Insert { key, .. } if key == "t-2"));
        assert!(matches!(&ops[2], ShapeOperation::UpToDate));
    }

    #[test]
    fn test_parse_ndjson_with_empty_lines() {
        let ndjson = r#"{"key":"t-1","value":{"id":"t-1"},"headers":{"operation":"insert"}}

{"headers":{"control":"up-to-date"}}
"#;

        let ops = ElectricClient::parse_ndjson(ndjson).unwrap();
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn test_parse_ndjson_empty() {
        let ndjson = "";
        let ops = ElectricClient::parse_ndjson(ndjson).unwrap();
        assert!(ops.is_empty());
    }

    // ==================
    // URL Building Tests
    // ==================

    #[test]
    fn test_build_url_initial_sync() {
        let config = ShapeConfig {
            base_url: "http://localhost:3000".to_string(),
            table: "tasks".to_string(),
            where_clause: None,
            columns: None,
        };
        let client = ElectricClient::new(config).unwrap();
        let state = ShapeState::initial();

        let url = client.build_url(&state, false);
        assert_eq!(url, "http://localhost:3000/v1/shape?table=tasks&offset=-1");
    }

    #[test]
    fn test_build_url_with_handle() {
        let config = ShapeConfig {
            base_url: "http://localhost:3000".to_string(),
            table: "tasks".to_string(),
            where_clause: None,
            columns: None,
        };
        let client = ElectricClient::new(config).unwrap();
        let state = ShapeState {
            handle: Some("12345-67890".to_string()),
            offset: "100_5".to_string(),
        };

        let url = client.build_url(&state, false);
        assert!(url.contains("handle=12345-67890"));
        assert!(url.contains("offset=100_5"));
    }

    #[test]
    fn test_build_url_with_where_clause() {
        let config = ShapeConfig {
            base_url: "http://localhost:3000".to_string(),
            table: "tasks".to_string(),
            where_clause: Some("project_id = $1".to_string()),
            columns: None,
        };
        let client = ElectricClient::new(config).unwrap();
        let state = ShapeState::initial();

        let url = client.build_url(&state, false);
        assert!(url.contains("where=project_id%20%3D%20%241"));
    }

    #[test]
    fn test_build_url_with_columns() {
        let config = ShapeConfig {
            base_url: "http://localhost:3000".to_string(),
            table: "tasks".to_string(),
            where_clause: None,
            columns: Some(vec![
                "id".to_string(),
                "title".to_string(),
                "status".to_string(),
            ]),
        };
        let client = ElectricClient::new(config).unwrap();
        let state = ShapeState::initial();

        let url = client.build_url(&state, false);
        assert!(url.contains("columns=id,title,status"));
    }

    #[test]
    fn test_build_url_live_mode() {
        let config = ShapeConfig {
            base_url: "http://localhost:3000".to_string(),
            table: "tasks".to_string(),
            where_clause: None,
            columns: None,
        };
        let client = ElectricClient::new(config).unwrap();
        let state = ShapeState {
            handle: Some("handle-123".to_string()),
            offset: "50_0".to_string(),
        };

        let url = client.build_url(&state, true);
        assert!(url.contains("live=true"));
    }

    // ==================
    // ShapeState Tests
    // ==================

    #[test]
    fn test_shape_state_initial() {
        let state = ShapeState::initial();
        assert_eq!(state.offset, "-1");
        assert!(state.handle.is_none());
    }

    // ==================
    // OperationType Serialization Tests
    // ==================

    #[test]
    fn test_operation_type_deserialize() {
        let json = r#""insert""#;
        let op: OperationType = serde_json::from_str(json).unwrap();
        assert_eq!(op, OperationType::Insert);

        let json = r#""update""#;
        let op: OperationType = serde_json::from_str(json).unwrap();
        assert_eq!(op, OperationType::Update);

        let json = r#""delete""#;
        let op: OperationType = serde_json::from_str(json).unwrap();
        assert_eq!(op, OperationType::Delete);
    }

    #[test]
    fn test_control_type_deserialize() {
        let json = r#""up-to-date""#;
        let ctrl: ControlType = serde_json::from_str(json).unwrap();
        assert_eq!(ctrl, ControlType::UpToDate);

        let json = r#""must-refetch""#;
        let ctrl: ControlType = serde_json::from_str(json).unwrap();
        assert_eq!(ctrl, ControlType::MustRefetch);
    }

    // ==================
    // Error Display Tests
    // ==================

    #[test]
    fn test_error_display() {
        let err = ElectricError::UnknownOperation("bad_op".to_string());
        assert_eq!(err.to_string(), "unknown operation type: bad_op");

        let err = ElectricError::MissingField("key".to_string());
        assert_eq!(err.to_string(), "missing required field: key");

        let err = ElectricError::Http {
            status: 500,
            body: "Internal error".to_string(),
        };
        assert_eq!(err.to_string(), "HTTP 500: Internal error");
    }
}
