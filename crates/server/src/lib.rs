#![allow(clippy::type_complexity)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::result_large_err)]

pub mod error;
pub mod file_logging;
pub mod mcp_http;
pub mod middleware;
pub mod relay_pairing;
pub mod routes;
pub mod runtime;
pub mod startup;

// #[cfg(feature = "cloud")]
// type DeploymentImpl = vibe_kanban_cloud::deployment::CloudDeployment;
// #[cfg(not(feature = "cloud"))]
pub type DeploymentImpl = local_deployment::LocalDeployment;
