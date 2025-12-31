//! Cached node model for storing node information synced from the hive (legacy implementation).
//!
//! # DEPRECATION NOTICE
//!
//! This module is **DEPRECATED** and its database table has been **DROPPED**.
//! The cached_nodes table no longer exists.
//!
//! The type definitions are kept for backwards compatibility with existing code
//! that still references these types. All database operations will return errors.
//!
//! ## Migration
//!
//! This module was removed as part of the explicit swarm linking migration.
//! Node information is now managed via the swarm management UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;
use uuid::Uuid;

/// Node status enum matching the remote server's NodeStatus
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CachedNodeStatus {
    #[default]
    Pending,
    Online,
    Offline,
    Busy,
    Draining,
}

impl std::fmt::Display for CachedNodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CachedNodeStatus::Pending => write!(f, "pending"),
            CachedNodeStatus::Online => write!(f, "online"),
            CachedNodeStatus::Offline => write!(f, "offline"),
            CachedNodeStatus::Busy => write!(f, "busy"),
            CachedNodeStatus::Draining => write!(f, "draining"),
        }
    }
}

impl std::str::FromStr for CachedNodeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(CachedNodeStatus::Pending),
            "online" => Ok(CachedNodeStatus::Online),
            "offline" => Ok(CachedNodeStatus::Offline),
            "busy" => Ok(CachedNodeStatus::Busy),
            "draining" => Ok(CachedNodeStatus::Draining),
            _ => Err(format!("Unknown node status: {}", s)),
        }
    }
}

impl From<String> for CachedNodeStatus {
    fn from(s: String) -> Self {
        s.parse().unwrap_or_default()
    }
}

/// Node capabilities describing what a node can execute
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct CachedNodeCapabilities {
    /// List of executor types this node supports
    #[serde(default)]
    pub executors: Vec<String>,
    /// Maximum number of concurrent tasks
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_tasks: i32,
    /// Operating system (e.g., "darwin", "linux", "windows")
    #[serde(default)]
    pub os: String,
    /// CPU architecture (e.g., "arm64", "x86_64")
    #[serde(default)]
    pub arch: String,
    /// Vibe Kanban version running on the node
    #[serde(default)]
    pub version: String,
}

fn default_max_concurrent() -> i32 {
    1
}

/// A cached node from the hive (legacy - table dropped).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CachedNode {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub machine_id: String,
    pub status: CachedNodeStatus,
    /// JSON-serialized capabilities
    #[serde(default)]
    capabilities_json: String,
    pub public_url: Option<String>,
    #[ts(type = "Date | null")]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    #[ts(type = "Date | null")]
    pub connected_at: Option<DateTime<Utc>>,
    #[ts(type = "Date | null")]
    pub disconnected_at: Option<DateTime<Utc>>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub last_synced_at: DateTime<Utc>,
}

impl CachedNode {
    /// Parse the capabilities from JSON
    pub fn capabilities(&self) -> CachedNodeCapabilities {
        serde_json::from_str(&self.capabilities_json).unwrap_or_default()
    }
}

/// Input for creating/updating a cached node
#[derive(Debug, Clone)]
pub struct CachedNodeInput {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub machine_id: String,
    pub status: CachedNodeStatus,
    pub capabilities: CachedNodeCapabilities,
    pub public_url: Option<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub connected_at: Option<DateTime<Utc>>,
    pub disconnected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn table_dropped_error() -> sqlx::Error {
    sqlx::Error::Protocol(
        "cached_nodes table has been dropped - use swarm management instead".to_string(),
    )
}

impl CachedNode {
    /// DEPRECATED: Table has been dropped.
    pub async fn list_by_organization(
        _pool: &SqlitePool,
        _organization_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn find_by_id(_pool: &SqlitePool, _id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn upsert(_pool: &SqlitePool, _data: CachedNodeInput) -> Result<Self, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn remove_stale(
        _pool: &SqlitePool,
        _organization_id: Uuid,
        _keep_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        Err(table_dropped_error())
    }

    /// DEPRECATED: Table has been dropped.
    pub async fn remove(_pool: &SqlitePool, _id: Uuid) -> Result<bool, sqlx::Error> {
        Err(table_dropped_error())
    }
}
