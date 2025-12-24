use axum::{
    Router,
    routing::{IntoMakeService, get},
};

use crate::DeploymentImpl;

pub mod all_tasks;
pub mod approvals;
pub mod backups;
pub mod config;
pub mod containers;
pub mod dashboard;
pub mod filesystem;
// pub mod github;
pub mod drafts;
pub mod events;
pub mod execution_processes;
pub mod frontend;
pub mod health;
pub mod images;
pub mod labels;
pub mod logs;
pub mod nodes;
pub mod oauth;
pub mod organizations;
pub mod processes;
pub mod projects;
pub mod shared_tasks;
pub mod templates;
pub mod terminal;
pub mod task_attempts;
pub mod task_variables;
pub mod tasks;

pub async fn router(deployment: DeploymentImpl) -> IntoMakeService<Router> {
    // Create terminal router with its own state
    let terminal_router = terminal::router_with_state(&deployment).await;

    // Create routers with different middleware layers
    let base_routes = Router::new()
        .route("/health", get(health::health_check))
        .merge(config::router())
        .merge(containers::router(&deployment))
        .merge(dashboard::router(&deployment))
        .merge(projects::router(&deployment))
        .merge(drafts::router(&deployment))
        .merge(tasks::router(&deployment))
        .merge(all_tasks::router(&deployment))
        .merge(shared_tasks::router())
        .merge(task_attempts::router(&deployment))
        .merge(execution_processes::router(&deployment))
        .merge(processes::router(&deployment))
        .merge(templates::router(&deployment))
        .merge(labels::router(&deployment))
        .merge(task_variables::router(&deployment))
        .merge(oauth::router())
        .merge(organizations::router())
        .merge(nodes::router())
        .merge(filesystem::router())
        .merge(events::router(&deployment))
        .merge(approvals::router())
        .merge(backups::router())
        .merge(logs::router(&deployment))
        .merge(terminal_router)
        .nest("/images", images::routes())
        .with_state(deployment);

    Router::new()
        .route("/", get(frontend::serve_frontend_root))
        .route("/{*path}", get(frontend::serve_frontend))
        .nest("/api", base_routes)
        .into_make_service()
}
