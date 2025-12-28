pub mod activity;
pub mod auth;
pub mod identity_errors;
pub mod invitations;
pub mod labels;
pub mod listener;
pub mod maintenance;
pub mod node_api_keys;
pub mod node_projects;
pub mod nodes;
pub mod oauth;
pub mod oauth_accounts;
pub mod organization_members;
pub mod organizations;
pub mod projects;
pub mod task_assignments;
pub mod task_output_logs;
pub mod task_progress_events;
pub mod tasks;
pub mod users;

pub use listener::ActivityListener;
use sqlx::{PgPool, Postgres, Transaction, migrate::MigrateError, postgres::PgPoolOptions};

pub(crate) type Tx<'a> = Transaction<'a, Postgres>;

/// Default number of PostgreSQL connections in the pool.
/// Can be overridden via the `VK_PG_MAX_CONNECTIONS` environment variable.
const DEFAULT_MAX_CONNECTIONS: u32 = 20;

/// Gets the maximum number of PostgreSQL connections from the environment.
///
/// Reads from `VK_PG_MAX_CONNECTIONS` environment variable. If not set or invalid,
/// returns the default of 20 connections (increased from the original 10 to better
/// support multi-node swarm architectures).
///
/// # Returns
/// - The value from `VK_PG_MAX_CONNECTIONS` if set and valid (positive integer)
/// - `DEFAULT_MAX_CONNECTIONS` (20) otherwise
pub fn get_max_connections() -> u32 {
    std::env::var("VK_PG_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_MAX_CONNECTIONS)
}

pub(crate) async fn migrate(pool: &PgPool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

pub(crate) async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(get_max_connections())
        .connect(database_url)
        .await
}
