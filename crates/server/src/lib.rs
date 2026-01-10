pub mod error;
pub mod file_logging;
pub mod mcp;
pub mod middleware;
pub mod proxy;
pub mod routes;
pub mod ws_util;

// #[cfg(feature = "cloud")]
// type DeploymentImpl = vibe_kanban_cloud::deployment::CloudDeployment;
// #[cfg(not(feature = "cloud"))]
pub type DeploymentImpl = local_deployment::LocalDeployment;
