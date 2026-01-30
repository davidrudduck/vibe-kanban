//! Merged view handlers for projects.
//!
//! This module contains handlers for the merged projects view:
//! - get_merged_projects: Combines local projects with swarm projects from Hive

use std::collections::HashMap;

use axum::{extract::State, response::Json as ResponseJson};
use db::models::{cached_node::CachedNode, project::Project};
use deployment::Deployment;
use remote::db::swarm_projects::SwarmProjectWithNodes;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

use super::super::types::{MergedProject, MergedProjectsResponse, NodeLocation, TaskCounts};
use super::core::truncate_node_name;

/// Get a merged view of all projects: local projects + swarm projects from Hive.
///
/// Projects with the same remote_project_id are merged into a single entry showing:
/// - has_local: true if a local copy exists
/// - nodes: list of remote nodes that have this project (from Hive)
///
/// Unlinked local projects (no remote_project_id) appear as standalone entries.
pub async fn get_merged_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<MergedProjectsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Get local projects with last attempt timestamp and task counts
    let local_projects_with_stats = Project::find_local_projects_with_stats(pool).await?;

    // Get current node_id to exclude from remote node list
    let current_node_id = if let Some(ctx) = deployment.node_runner_context() {
        ctx.node_id().await
    } else {
        None
    };

    // Load cached nodes to build node details (status, public_url, OS)
    let cached_nodes = match CachedNode::list_all(pool).await {
        Ok(nodes) => nodes,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load cached nodes");
            Vec::new()
        }
    };
    let cached_nodes_map: HashMap<Uuid, CachedNode> =
        cached_nodes.into_iter().map(|n| (n.id, n)).collect();

    // Fetch swarm projects from Hive directly (source of truth for remote projects)
    let swarm_projects: Vec<SwarmProjectWithNodes> =
        if let Some(ctx) = deployment.node_runner_context() {
            if let Some(org_id) = ctx.organization_id().await {
                // Prefer node_auth_client (API key auth) - works even without user login
                // Fall back to remote_client (OAuth) for non-node deployments
                let client_opt = deployment
                    .node_auth_client()
                    .cloned()
                    .or_else(|| deployment.remote_client().ok());

                match client_opt {
                    Some(client) => match client.list_swarm_projects(org_id).await {
                        Ok(response) => {
                            tracing::debug!(
                                swarm_project_count = response.projects.len(),
                                "loaded swarm projects from hive"
                            );
                            response.projects
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "Failed to fetch swarm projects from hive");
                            Vec::new()
                        }
                    },
                    None => {
                        tracing::debug!("No remote client available for swarm projects");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

    // Build lookup from swarm_project_id -> SwarmProjectWithNodes
    let swarm_projects_map: HashMap<Uuid, &SwarmProjectWithNodes> = swarm_projects
        .iter()
        .map(|sp| (sp.project.id, sp))
        .collect();

    // Build a map from remote_project_id -> MergedProject
    let mut merged_map: HashMap<Uuid, MergedProject> = HashMap::new();

    // Track local-only projects (those without remote_project_id)
    let mut local_only_projects: Vec<MergedProject> = Vec::new();

    // Process local projects first
    for stats in local_projects_with_stats {
        let project = stats.project;
        let last_attempt_at = stats.last_attempt_at;
        let task_counts = TaskCounts {
            todo: stats.task_counts.todo,
            in_progress: stats.task_counts.in_progress,
            in_review: stats.task_counts.in_review,
            done: stats.task_counts.done,
        };

        if let Some(remote_project_id) = project.remote_project_id {
            // This local project is linked to a swarm project
            // Build node locations from swarm project's linked_node_ids (excluding current node)
            let nodes = if let Some(swarm_project) = swarm_projects_map.get(&remote_project_id) {
                build_node_locations(
                    swarm_project,
                    &cached_nodes_map,
                    current_node_id,
                    remote_project_id,
                )
            } else {
                Vec::new()
            };

            merged_map.insert(
                remote_project_id,
                MergedProject {
                    id: project.id,
                    name: project.name.clone(),
                    git_repo_path: project.git_repo_path.to_string_lossy().to_string(),
                    created_at: project.created_at,
                    remote_project_id: Some(remote_project_id),
                    has_local: true,
                    local_project_id: Some(project.id),
                    nodes,
                    last_attempt_at,
                    github_enabled: project.github_enabled,
                    github_owner: project.github_owner.clone(),
                    github_repo: project.github_repo.clone(),
                    github_open_issues: project.github_open_issues,
                    github_open_prs: project.github_open_prs,
                    github_last_synced_at: project.github_last_synced_at,
                    task_counts,
                },
            );
        } else {
            // This local project is not linked - add it as standalone
            local_only_projects.push(MergedProject {
                id: project.id,
                name: project.name.clone(),
                git_repo_path: project.git_repo_path.to_string_lossy().to_string(),
                created_at: project.created_at,
                remote_project_id: None,
                has_local: true,
                local_project_id: Some(project.id),
                nodes: Vec::new(),
                last_attempt_at,
                github_enabled: project.github_enabled,
                github_owner: project.github_owner.clone(),
                github_repo: project.github_repo.clone(),
                github_open_issues: project.github_open_issues,
                github_open_prs: project.github_open_prs,
                github_last_synced_at: project.github_last_synced_at,
                task_counts,
            });
        }
    }

    // Process swarm projects that don't have a local copy
    for swarm_project in &swarm_projects {
        let swarm_project_id = swarm_project.project.id;

        // Skip if we already processed this as a local project
        if merged_map.contains_key(&swarm_project_id) {
            continue;
        }

        // Check if current node is linked to this swarm project
        let has_current_node =
            current_node_id.is_some_and(|nid| swarm_project.linked_node_ids.contains(&nid));

        // Skip swarm projects that only have current node linked (no remote nodes)
        if has_current_node && swarm_project.linked_node_ids.len() == 1 {
            continue;
        }

        // Build node locations from linked nodes (excluding current node)
        let nodes = build_node_locations(
            swarm_project,
            &cached_nodes_map,
            current_node_id,
            swarm_project_id,
        );

        // Skip if no remote nodes available
        if nodes.is_empty() {
            continue;
        }

        // Task counts from swarm project
        let task_counts = TaskCounts {
            todo: swarm_project.task_counts.todo as i32,
            in_progress: swarm_project.task_counts.in_progress as i32,
            in_review: swarm_project.task_counts.in_review as i32,
            done: swarm_project.task_counts.done as i32,
        };

        // Create entry for remote-only project
        merged_map.insert(
            swarm_project_id,
            MergedProject {
                id: swarm_project_id,
                name: swarm_project.project.name.clone(),
                // Use first node's path if available, otherwise empty
                git_repo_path: String::new(),
                created_at: swarm_project.project.created_at,
                remote_project_id: Some(swarm_project_id),
                has_local: false,
                local_project_id: None,
                nodes,
                last_attempt_at: None, // Remote projects don't have local attempt data
                // Remote projects don't have GitHub integration
                github_enabled: false,
                github_owner: None,
                github_repo: None,
                github_open_issues: 0,
                github_open_prs: 0,
                github_last_synced_at: None,
                task_counts,
            },
        );
    }

    // Combine all projects into a single list
    let mut projects: Vec<MergedProject> = merged_map.into_values().collect();
    projects.extend(local_only_projects);

    // Sort by name (default sort) - frontend will handle other sort options
    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    tracing::debug!(
        project_count = projects.len(),
        "merged projects: returning projects"
    );

    Ok(ResponseJson(ApiResponse::success(MergedProjectsResponse {
        projects,
    })))
}

/// Build NodeLocation entries from a swarm project's linked nodes
fn build_node_locations(
    swarm_project: &SwarmProjectWithNodes,
    cached_nodes_map: &HashMap<Uuid, CachedNode>,
    current_node_id: Option<Uuid>,
    remote_project_id: Uuid,
) -> Vec<NodeLocation> {
    swarm_project
        .linked_node_ids
        .iter()
        .filter(|&node_id| {
            // Exclude current node from the list
            current_node_id != Some(*node_id)
        })
        .map(|&node_id| {
            // Try to get node details from cache
            if let Some(cached_node) = cached_nodes_map.get(&node_id) {
                NodeLocation {
                    node_id,
                    node_name: cached_node.name.clone(),
                    node_short_name: truncate_node_name(&cached_node.name),
                    node_status: cached_node.status,
                    node_public_url: cached_node.public_url.clone(),
                    remote_project_id,
                    node_os: Some(cached_node.capabilities().os.clone()),
                }
            } else {
                // Node not in cache - try to find name from swarm project
                // linked_node_names is parallel to linked_node_ids
                let name_index = swarm_project
                    .linked_node_ids
                    .iter()
                    .position(|id| *id == node_id);
                let node_name = name_index
                    .and_then(|i| swarm_project.linked_node_names.get(i))
                    .cloned()
                    .unwrap_or_else(|| format!("Node {}", &node_id.to_string()[..8]));

                NodeLocation {
                    node_id,
                    node_name: node_name.clone(),
                    node_short_name: truncate_node_name(&node_name),
                    node_status: db::models::cached_node::CachedNodeStatus::Offline,
                    node_public_url: None,
                    remote_project_id,
                    node_os: None,
                }
            }
        })
        .collect()
}
