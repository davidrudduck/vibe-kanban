//! Merged view handlers for projects.
//!
//! This module contains handlers for the merged projects view:
//! - get_merged_projects: Combines local and remote projects by remote_project_id

use std::collections::HashMap;

use axum::{extract::State, response::Json as ResponseJson};
use db::models::{cached_node::CachedNode, project::Project};
use deployment::Deployment;
use remote::db::swarm_projects::SwarmTaskCounts;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

use super::super::types::{MergedProject, MergedProjectsResponse, NodeLocation, TaskCounts};
use super::core::truncate_node_name;

/// Get a merged view of all projects: local and remote projects merged by remote_project_id.
///
/// Projects with the same remote_project_id are merged into a single entry showing:
/// - has_local: true if a local copy exists
/// - nodes: list of remote nodes that have this project
///
/// Unlinked local projects (no remote_project_id) appear as standalone entries.
pub async fn get_merged_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<MergedProjectsResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Get local projects with last attempt timestamp and task counts
    let local_projects_with_stats = Project::find_local_projects_with_stats(pool).await?;

    // Get current node_id to exclude from remote list (if connected to hive)
    let current_node_id = if let Some(ctx) = deployment.node_runner_context() {
        ctx.node_id().await
    } else {
        None
    };

    // Get all remote projects (from other nodes)
    let all_remote = match Project::find_remote_projects(pool).await {
        Ok(projects) => projects,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load remote projects");
            Vec::new()
        }
    };

    // Load cached nodes to get OS info for each node
    let cached_nodes = match CachedNode::list_all(pool).await {
        Ok(nodes) => nodes,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load cached nodes for OS info");
            Vec::new()
        }
    };
    let node_os_map: HashMap<Uuid, String> = cached_nodes
        .into_iter()
        .map(|n| (n.id, n.capabilities().os))
        .collect();

    // Filter out projects from current node
    let all_remote: Vec<_> = all_remote
        .into_iter()
        .filter(|p| {
            if let Some(current_id) = current_node_id {
                p.source_node_id != Some(current_id)
            } else {
                true
            }
        })
        .collect();

    // Fetch swarm projects with task counts from Hive
    // Build a map from hive_project_id → task counts for remote project lookup
    let swarm_task_counts: HashMap<Uuid, SwarmTaskCounts> = if let Some(ctx) =
        deployment.node_runner_context()
    {
        if let Some(org_id) = ctx.organization_id().await {
            match deployment.remote_client() {
                Ok(client) => match client.list_swarm_projects(org_id).await {
                    Ok(response) => {
                        let mut map = HashMap::new();
                        for swarm_project in response.projects {
                            // Map each hive_project_id to the swarm project's task counts
                            for hive_id in swarm_project.hive_project_ids {
                                map.insert(hive_id, swarm_project.task_counts.clone());
                            }
                        }
                        tracing::debug!(
                            swarm_project_count = map.len(),
                            "loaded swarm project task counts"
                        );
                        map
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "Failed to fetch swarm projects for task counts");
                        HashMap::new()
                    }
                },
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    // Build a map from remote_project_id -> MergedProject
    // Key: remote_project_id (Uuid) -> Value: MergedProject being built
    let mut merged_map: HashMap<Uuid, MergedProject> = HashMap::new();

    // Also track local-only projects (those without remote_project_id)
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
            // This local project is linked to a remote project
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
                    nodes: Vec::new(), // Will be populated from remote projects
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

    // Process remote projects - merge into existing or create new entries
    for remote_project in all_remote {
        // Use remote_project_id if available, otherwise use local ID as fallback key.
        // This ensures remote projects without remote_project_id still appear in the list.
        let merge_key = remote_project
            .remote_project_id
            .unwrap_or(remote_project.id);

        let source_node_id = remote_project.source_node_id.unwrap_or_default();
        let node_location = NodeLocation {
            node_id: source_node_id,
            node_name: remote_project.source_node_name.clone().unwrap_or_default(),
            node_short_name: truncate_node_name(
                remote_project.source_node_name.as_deref().unwrap_or(""),
            ),
            node_status: remote_project
                .source_node_status
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(db::models::cached_node::CachedNodeStatus::Pending),
            node_public_url: remote_project.source_node_public_url.clone(),
            // Use the actual remote_project_id for node location, falling back to local ID
            remote_project_id: remote_project
                .remote_project_id
                .unwrap_or(remote_project.id),
            // Lookup OS from cached node capabilities
            node_os: node_os_map.get(&source_node_id).cloned(),
        };

        if let Some(merged) = merged_map.get_mut(&merge_key) {
            // Add this node to existing merged project
            merged.nodes.push(node_location);
        } else {
            // Look up task counts from swarm projects using the hive project ID
            let task_counts = remote_project
                .remote_project_id
                .and_then(|hive_id| swarm_task_counts.get(&hive_id))
                .map(|swarm_counts| TaskCounts {
                    todo: swarm_counts.todo as i32,
                    in_progress: swarm_counts.in_progress as i32,
                    in_review: swarm_counts.in_review as i32,
                    done: swarm_counts.done as i32,
                })
                .unwrap_or_default();

            // Create new entry for remote-only project
            merged_map.insert(
                merge_key,
                MergedProject {
                    id: remote_project.id, // Use remote project's local ID
                    name: remote_project.name.clone(),
                    git_repo_path: remote_project.git_repo_path.to_string_lossy().to_string(),
                    created_at: remote_project.created_at,
                    // Preserve the actual remote_project_id (may be None for legacy data)
                    remote_project_id: remote_project.remote_project_id,
                    has_local: false,
                    local_project_id: None,
                    nodes: vec![node_location],
                    last_attempt_at: None, // Remote projects don't have local attempt data
                    // Remote projects don't have GitHub integration
                    github_enabled: false,
                    github_owner: None,
                    github_repo: None,
                    github_open_issues: 0,
                    github_open_prs: 0,
                    github_last_synced_at: None,
                    // Task counts from swarm project (fetched from Hive)
                    task_counts,
                },
            );
        }
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
