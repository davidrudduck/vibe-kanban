use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use ts_rs::TS;

/// Pool-level connection statistics for the SQLite connection pool.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PoolStats {
    /// Total connections in the pool (idle + acquired)
    pub size: u32,
    /// Connections currently idle (available)
    pub idle: u32,
    /// Connections currently acquired (in use)
    pub acquired: u32,
}

/// Snapshot pool stats from a live SQLite pool.
pub fn pool_stats(pool: &Pool<Sqlite>) -> PoolStats {
    let size = pool.size();
    let idle = pool.num_idle() as u32;
    PoolStats {
        size,
        idle,
        acquired: size.saturating_sub(idle),
    }
}
