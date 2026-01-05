use std::{future::Future, path::PathBuf, str::FromStr};

use db::models::{
    execution_process::ExecutionProcess,
    label::Label,
    project::Project,
    task::{CreateTask, Task, TaskStatus, TaskWithAttemptStatus, UpdateTask},
    task_attempt::{TaskAttempt, TaskAttemptContext},
    task_variable::{ResolvedVariable, TaskVariable},
};
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json;
use uuid::Uuid;

use remote::routes::projects::{ListProjectNodesResponse, ProjectNodeInfo};

use crate::routes::{containers::ContainerQuery, task_attempts::CreateTaskAttemptBody};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskRequest {
    #[schemars(description = "The ID of the project to create the task in. This is required!")]
    pub project_id: Uuid,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    #[serde(default)]
    pub description: Option<String>,
    #[schemars(
        description = "Link as subtask of a parent task. If true and in context of an attempt, auto-links to current task."
    )]
    #[serde(default)]
    pub link_to_parent: Option<bool>,
    #[schemars(description = "Explicit parent task ID. Overrides link_to_parent.")]
    #[serde(default)]
    pub parent_task_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreateTaskResponse {
    pub task_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectSummary {
    #[schemars(description = "The unique identifier of the project")]
    pub id: String,
    #[schemars(description = "The name of the project")]
    pub name: String,
    #[schemars(description = "The path to the git repository")]
    pub git_repo_path: PathBuf,
    #[schemars(description = "Optional setup script for the project")]
    pub setup_script: Option<String>,
    #[schemars(description = "Optional cleanup script for the project")]
    pub cleanup_script: Option<String>,
    #[schemars(description = "Optional development script for the project")]
    pub dev_script: Option<String>,
    #[schemars(description = "When the project was created")]
    pub created_at: String,
    #[schemars(description = "When the project was last updated")]
    pub updated_at: String,
}

impl ProjectSummary {
    fn from_project(project: Project) -> Self {
        Self {
            id: project.id.to_string(),
            name: project.name,
            git_repo_path: project.git_repo_path,
            setup_script: project.setup_script,
            cleanup_script: project.cleanup_script,
            dev_script: project.dev_script,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectSummary>,
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksRequest {
    #[schemars(description = "The ID of the project to list tasks from")]
    pub project_id: Uuid,
    #[schemars(
        description = "Optional status filter: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'"
    )]
    pub status: Option<String>,
    #[schemars(description = "Maximum number of tasks to return (default: 50)")]
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskSummary {
    #[schemars(description = "The unique identifier of the task")]
    pub id: String,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Current status of the task")]
    pub status: String,
    #[schemars(description = "When the task was created")]
    pub created_at: String,
    #[schemars(description = "When the task was last updated")]
    pub updated_at: String,
    #[schemars(description = "Whether the task has an in-progress execution attempt")]
    pub has_in_progress_attempt: Option<bool>,
    #[schemars(description = "Whether the task has a merged execution attempt")]
    pub has_merged_attempt: Option<bool>,
    #[schemars(description = "Whether the last execution attempt failed")]
    pub last_attempt_failed: Option<bool>,
}

impl TaskSummary {
    fn from_task_with_status(task: TaskWithAttemptStatus) -> Self {
        Self {
            id: task.id.to_string(),
            title: task.title.to_string(),
            status: task.status.to_string(),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
            has_in_progress_attempt: Some(task.has_in_progress_attempt),
            has_merged_attempt: Some(task.has_merged_attempt),
            last_attempt_failed: Some(task.last_attempt_failed),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskDetails {
    #[schemars(description = "The unique identifier of the task")]
    pub id: String,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(description = "Current status of the task")]
    pub status: String,
    #[schemars(description = "When the task was created")]
    pub created_at: String,
    #[schemars(description = "When the task was last updated")]
    pub updated_at: String,
    #[schemars(description = "Whether the task has an in-progress execution attempt")]
    pub has_in_progress_attempt: Option<bool>,
    #[schemars(description = "Whether the task has a merged execution attempt")]
    pub has_merged_attempt: Option<bool>,
    #[schemars(description = "Whether the last execution attempt failed")]
    pub last_attempt_failed: Option<bool>,
}

impl TaskDetails {
    fn from_task(task: Task) -> Self {
        Self {
            id: task.id.to_string(),
            title: task.title,
            description: task.description,
            status: task.status.to_string(),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
            has_in_progress_attempt: None,
            has_merged_attempt: None,
            last_attempt_failed: None,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListTasksResponse {
    pub tasks: Vec<TaskSummary>,
    pub count: usize,
    pub project_id: String,
    pub applied_filters: ListTasksFilters,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListTasksFilters {
    pub status: Option<String>,
    pub limit: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskRequest {
    #[schemars(description = "The ID of the task to update")]
    pub task_id: Uuid,
    #[schemars(description = "New title for the task")]
    pub title: Option<String>,
    #[schemars(description = "New description for the task")]
    pub description: Option<String>,
    #[schemars(description = "New status: 'todo', 'inprogress', 'inreview', 'done', 'cancelled'")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UpdateTaskResponse {
    pub task: TaskDetails,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteTaskRequest {
    #[schemars(description = "The ID of the task to delete")]
    pub task_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StartTaskAttemptRequest {
    #[schemars(description = "The ID of the task to start")]
    pub task_id: Uuid,
    #[schemars(
        description = "The coding agent executor to run ('CLAUDE_CODE', 'CODEX', 'GEMINI', 'CURSOR_AGENT', 'OPENCODE')"
    )]
    pub executor: String,
    #[schemars(description = "Optional executor variant, if needed")]
    pub variant: Option<String>,
    #[schemars(description = "The base branch to use for the attempt")]
    pub base_branch: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StartTaskAttemptResponse {
    pub task_id: String,
    pub attempt_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeleteTaskResponse {
    pub deleted_task_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskRequest {
    #[schemars(description = "The ID of the task to retrieve")]
    pub task_id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetContextRequest {
    #[schemars(
        description = "Working directory path to identify the task attempt. Pass the absolute path where you are executing commands."
    )]
    pub cwd: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CwdRequest {
    #[schemars(
        description = "Working directory path to identify the task attempt. Pass the absolute path where you are executing commands."
    )]
    pub cwd: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskIdResponse {
    #[schemars(description = "The task ID for the current task attempt")]
    pub task_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectIdResponse {
    #[schemars(description = "The project ID for the current task attempt")]
    pub project_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskResponse {
    pub task: TaskDetails,
}

// ===== Task Variables MCP Types =====

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskVariablesRequest {
    #[schemars(description = "The ID of the task to get variables for")]
    pub task_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskVariableDetails {
    #[schemars(description = "The variable name")]
    pub name: String,
    #[schemars(description = "The variable value")]
    pub value: String,
    #[schemars(description = "The task ID where this variable was defined")]
    pub source_task_id: String,
    #[schemars(description = "True if this variable was inherited from a parent task")]
    pub inherited: bool,
}

impl TaskVariableDetails {
    fn from_resolved(rv: ResolvedVariable) -> Self {
        Self {
            name: rv.name,
            value: rv.value,
            source_task_id: rv.source_task_id.to_string(),
            inherited: rv.inherited,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskVariablesResponse {
    #[schemars(description = "The task ID these variables belong to")]
    pub task_id: String,
    #[schemars(description = "The resolved variables (including inherited from parent tasks)")]
    pub variables: Vec<TaskVariableDetails>,
    #[schemars(description = "Number of variables returned")]
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetTaskVariableRequest {
    #[schemars(description = "The ID of the task to set the variable on")]
    pub task_id: Uuid,
    #[schemars(
        description = "The variable name. Must start with uppercase letter and contain only uppercase letters, digits, and underscores (e.g., MY_VAR, API_KEY_2)"
    )]
    pub name: String,
    #[schemars(description = "The variable value")]
    pub value: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SetTaskVariableResponse {
    #[schemars(description = "The variable ID")]
    pub id: String,
    #[schemars(description = "The variable name")]
    pub name: String,
    #[schemars(description = "The variable value")]
    pub value: String,
    #[schemars(description = "The task ID this variable was set on")]
    pub task_id: String,
    #[schemars(description = "Whether a new variable was created (false if existing was updated)")]
    pub created: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteTaskVariableRequest {
    #[schemars(description = "The ID of the task to delete the variable from")]
    pub task_id: Uuid,
    #[schemars(description = "The name of the variable to delete")]
    pub name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeleteTaskVariableResponse {
    #[schemars(description = "The name of the deleted variable")]
    pub deleted_variable_name: String,
    #[schemars(description = "The task ID the variable was deleted from")]
    pub task_id: String,
}

// ===== Task Attempt Execution Control MCP Types =====

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StopTaskAttemptRequest {
    #[schemars(description = "The ID of the task attempt to stop")]
    pub attempt_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StopTaskAttemptResponse {
    #[schemars(description = "The ID of the task attempt that was stopped")]
    pub attempt_id: String,
    #[schemars(description = "Whether the stop operation was successful")]
    pub stopped: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskAttemptStatusRequest {
    #[schemars(description = "The ID of the task attempt to get status for")]
    pub attempt_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ExecutionProcessSummary {
    #[schemars(description = "The unique identifier of the execution process")]
    pub id: String,
    #[schemars(description = "The reason this process was run (e.g., 'initial', 'follow_up')")]
    pub run_reason: String,
    #[schemars(description = "Current status of the execution process")]
    pub status: String,
    #[schemars(description = "When the process started")]
    pub started_at: String,
    #[schemars(description = "When the process completed (if finished)")]
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskAttemptStatusResponse {
    #[schemars(description = "The unique identifier of the task attempt")]
    pub attempt_id: String,
    #[schemars(description = "The ID of the task this attempt belongs to")]
    pub task_id: String,
    #[schemars(description = "The git branch name for this attempt")]
    pub branch: String,
    #[schemars(description = "The executor used for this attempt")]
    pub executor: String,
    #[schemars(description = "Whether the worktree has been deleted")]
    pub worktree_deleted: bool,
    #[schemars(description = "When the attempt was created")]
    pub created_at: String,
    #[schemars(description = "List of execution processes for this attempt")]
    pub processes: Vec<ExecutionProcessSummary>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTaskAttemptsRequest {
    #[schemars(description = "The ID of the task to list attempts for")]
    pub task_id: Uuid,
    #[schemars(description = "Maximum number of attempts to return (default: 50)")]
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskAttemptSummary {
    #[schemars(description = "The unique identifier of the task attempt")]
    pub id: String,
    #[schemars(description = "The git branch name for this attempt")]
    pub branch: String,
    #[schemars(description = "The executor used for this attempt")]
    pub executor: String,
    #[schemars(description = "When the attempt was created")]
    pub created_at: String,
    #[schemars(description = "Whether the worktree has been deleted")]
    pub worktree_deleted: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListTaskAttemptsResponse {
    #[schemars(description = "The ID of the task these attempts belong to")]
    pub task_id: String,
    #[schemars(description = "List of task attempts")]
    pub attempts: Vec<TaskAttemptSummary>,
    #[schemars(description = "Number of attempts returned")]
    pub count: usize,
}

// ===== Label MCP Types =====

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct LabelSummary {
    #[schemars(description = "The unique identifier of the label")]
    pub id: String,
    #[schemars(description = "The name of the label")]
    pub name: String,
    #[schemars(description = "Lucide icon name (e.g., 'tag', 'bug', 'code')")]
    pub icon: String,
    #[schemars(description = "Hex color code (e.g., '#3b82f6')")]
    pub color: String,
}

impl LabelSummary {
    fn from_label(label: Label) -> Self {
        Self {
            id: label.id.to_string(),
            name: label.name,
            icon: label.icon,
            color: label.color,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskLabelsRequest {
    #[schemars(description = "The ID of the task to get labels for")]
    pub task_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct GetTaskLabelsResponse {
    #[schemars(description = "The ID of the task")]
    pub task_id: String,
    #[schemars(description = "Labels assigned to the task")]
    pub labels: Vec<LabelSummary>,
    #[schemars(description = "Number of labels returned")]
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetTaskLabelsRequest {
    #[schemars(description = "The ID of the task to set labels for")]
    pub task_id: Uuid,
    #[schemars(description = "List of label IDs to assign to the task (replaces existing labels)")]
    pub label_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SetTaskLabelsResponse {
    #[schemars(description = "The ID of the task")]
    pub task_id: String,
    #[schemars(description = "Labels now assigned to the task")]
    pub labels: Vec<LabelSummary>,
    #[schemars(description = "Number of labels set")]
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListLabelsRequest {
    #[schemars(
        description = "Optional project ID. If provided, returns global + project-specific labels. If not, returns only global labels."
    )]
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListLabelsResponse {
    #[schemars(description = "Available labels")]
    pub labels: Vec<LabelSummary>,
    #[schemars(description = "Number of labels returned")]
    pub count: usize,
    #[schemars(description = "Project ID filter that was applied (null if global only)")]
    pub project_id: Option<String>,
}

// ===== Nodes MCP Types =====

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListNodesRequest {
    #[schemars(description = "The ID of the task to list available nodes for")]
    pub task_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct NodeSummary {
    #[schemars(description = "The unique identifier of the node")]
    pub node_id: String,
    #[schemars(description = "The name of the node")]
    pub node_name: String,
    #[schemars(description = "Current status of the node (pending, online, offline, busy, draining)")]
    pub status: String,
    #[schemars(description = "Public URL for direct connection to the node (if available)")]
    pub public_url: Option<String>,
}

impl NodeSummary {
    fn from_project_node_info(info: ProjectNodeInfo) -> Self {
        Self {
            node_id: info.node_id.to_string(),
            node_name: info.node_name,
            status: format!("{:?}", info.node_status).to_lowercase(),
            public_url: info.node_public_url,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListNodesResponse {
    #[schemars(description = "The ID of the task")]
    pub task_id: String,
    #[schemars(description = "Available nodes that have this task's project linked")]
    pub nodes: Vec<NodeSummary>,
    #[schemars(description = "Number of nodes returned")]
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct TaskServer {
    client: reqwest::Client,
    base_url: String,
    tool_router: ToolRouter<TaskServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpContext {
    pub project_id: Uuid,
    pub task_id: Uuid,
    pub task_title: String,
    pub attempt_id: Uuid,
    pub attempt_branch: String,
    pub attempt_target_branch: String,
    pub executor: String,
}

impl TaskServer {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            tool_router: Self::tool_router(),
        }
    }

    /// Initialize the server.
    ///
    /// The get_context tool requires a `cwd` parameter to identify the task attempt,
    /// so it's always registered and context is fetched dynamically per-request.
    pub async fn init(self) -> Self {
        tracing::info!("MCP server initialized, get_context tool requires cwd parameter");
        self
    }

    /// Fetch context for a specific path by looking up the task attempt
    /// that has this path as its container_ref.
    async fn fetch_context_for_path(&self, path: &str) -> Option<McpContext> {
        let url = self.url("/api/containers/attempt-context");
        let query = ContainerQuery {
            container_ref: path.to_string(),
        };

        let response = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.client.get(&url).query(&query).send(),
        )
        .await
        .ok()?
        .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let api_response: ApiResponseEnvelope<TaskAttemptContext> = response.json().await.ok()?;

        if !api_response.success {
            return None;
        }

        let ctx = api_response.data?;
        Some(McpContext {
            project_id: ctx.project.id,
            task_id: ctx.task.id,
            task_title: ctx.task.title,
            attempt_id: ctx.task_attempt.id,
            attempt_branch: ctx.task_attempt.branch,
            attempt_target_branch: ctx.task_attempt.target_branch,
            executor: ctx.task_attempt.executor,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponseEnvelope<T> {
    success: bool,
    data: Option<T>,
    message: Option<String>,
}

impl TaskServer {
    fn success<T: Serialize>(data: &T) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(data)
                .unwrap_or_else(|_| "Failed to serialize response".to_string()),
        )]))
    }

    fn err_value(v: serde_json::Value) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::error(vec![Content::text(
            serde_json::to_string_pretty(&v)
                .unwrap_or_else(|_| "Failed to serialize error".to_string()),
        )]))
    }

    fn err<S: Into<String>>(msg: S, details: Option<S>) -> Result<CallToolResult, ErrorData> {
        let mut v = serde_json::json!({"success": false, "error": msg.into()});
        if let Some(d) = details {
            v["details"] = serde_json::json!(d.into());
        };
        Self::err_value(v)
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        rb: reqwest::RequestBuilder,
    ) -> Result<T, CallToolResult> {
        let resp = rb
            .send()
            .await
            .map_err(|e| Self::err("Failed to connect to VK API", Some(&e.to_string())).unwrap())?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(
                Self::err(format!("VK API returned error status: {}", status), None).unwrap(),
            );
        }

        let api_response = resp.json::<ApiResponseEnvelope<T>>().await.map_err(|e| {
            Self::err("Failed to parse VK API response", Some(&e.to_string())).unwrap()
        })?;

        if !api_response.success {
            let msg = api_response.message.as_deref().unwrap_or("Unknown error");
            return Err(Self::err("VK API returned error", Some(msg)).unwrap());
        }

        api_response
            .data
            .ok_or_else(|| Self::err("VK API response missing data field", None).unwrap())
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

#[tool_router]
impl TaskServer {
    #[tool(description = "Get current task attempt context.")]
    async fn get_context(
        &self,
        Parameters(GetContextRequest { cwd }): Parameters<GetContextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Fetch context for the provided working directory path
        if let Some(ctx) = self.fetch_context_for_path(&cwd).await {
            return TaskServer::success(&ctx);
        }
        TaskServer::err(
            &format!(
                "No task attempt found for path: {}. Ensure you're in a vibe-kanban task attempt worktree.",
                cwd
            ),
            None,
        )
    }
    #[tool(description = "Get task ID for the current task attempt. Lightweight alternative to get_context.")]
    async fn get_task_id(
        &self,
        Parameters(CwdRequest { cwd }): Parameters<CwdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(ctx) = self.fetch_context_for_path(&cwd).await {
            return TaskServer::success(&TaskIdResponse {
                task_id: ctx.task_id.to_string(),
            });
        }
        TaskServer::err(
            &format!(
                "No task attempt found for path: {}. Ensure you're in a vibe-kanban task attempt worktree.",
                cwd
            ),
            None,
        )
    }

    #[tool(description = "Get project ID for the current task attempt. Lightweight alternative to get_context.")]
    async fn get_project_id(
        &self,
        Parameters(CwdRequest { cwd }): Parameters<CwdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(ctx) = self.fetch_context_for_path(&cwd).await {
            return TaskServer::success(&ProjectIdResponse {
                project_id: ctx.project_id.to_string(),
            });
        }
        TaskServer::err(
            &format!(
                "No task attempt found for path: {}. Ensure you're in a vibe-kanban task attempt worktree.",
                cwd
            ),
            None,
        )
    }

    #[tool(description = "Create task. Requires project_id.")]
    async fn create_task(
        &self,
        Parameters(CreateTaskRequest {
            project_id,
            title,
            description,
            link_to_parent,
            parent_task_id,
        }): Parameters<CreateTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Determine parent_task_id: explicit value takes precedence
        // Note: link_to_parent requires parent_task_id to be set explicitly
        // (use get_context with cwd to get the current task_id first)
        let resolved_parent_task_id = if let Some(explicit) = parent_task_id {
            Some(explicit)
        } else if link_to_parent.unwrap_or(false) {
            // link_to_parent was requested but no parent_task_id provided
            return TaskServer::err(
                "link_to_parent requires parent_task_id. Use get_context(cwd) first to get the current task_id.",
                None,
            );
        } else {
            None
        };

        let url = self.url("/api/tasks");
        let create_payload = CreateTask {
            project_id,
            title,
            description,
            status: Some(TaskStatus::Todo),
            parent_task_id: resolved_parent_task_id,
            image_ids: None,
            shared_task_id: None,
        };

        let task: Task = match self
            .send_json(self.client.post(&url).json(&create_payload))
            .await
        {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        TaskServer::success(&CreateTaskResponse {
            task_id: task.id.to_string(),
        })
    }

    #[tool(description = "List all projects.")]
    async fn list_projects(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/projects");
        let projects: Vec<Project> = match self.send_json(self.client.get(&url)).await {
            Ok(ps) => ps,
            Err(e) => return Ok(e),
        };

        let project_summaries: Vec<ProjectSummary> = projects
            .into_iter()
            .map(ProjectSummary::from_project)
            .collect();

        let response = ListProjectsResponse {
            count: project_summaries.len(),
            projects: project_summaries,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "List tasks. Requires project_id.")]
    async fn list_tasks(
        &self,
        Parameters(ListTasksRequest {
            project_id,
            status,
            limit,
        }): Parameters<ListTasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let status_filter = if let Some(ref status_str) = status {
            match TaskStatus::from_str(status_str) {
                Ok(s) => Some(s),
                Err(_) => {
                    return Self::err(
                        "Invalid status filter. Valid values: 'todo', 'in-progress', 'in-review', 'done', 'cancelled'".to_string(),
                        Some(status_str.to_string()),
                    );
                }
            }
        } else {
            None
        };

        let url = self.url(&format!("/api/tasks?project_id={}", project_id));
        let all_tasks: Vec<TaskWithAttemptStatus> =
            match self.send_json(self.client.get(&url)).await {
                Ok(t) => t,
                Err(e) => return Ok(e),
            };

        let task_limit = limit.unwrap_or(50).max(0) as usize;
        let filtered = all_tasks.into_iter().filter(|t| {
            if let Some(ref want) = status_filter {
                &t.status == want
            } else {
                true
            }
        });
        let limited: Vec<TaskWithAttemptStatus> = filtered.take(task_limit).collect();

        let task_summaries: Vec<TaskSummary> = limited
            .into_iter()
            .map(TaskSummary::from_task_with_status)
            .collect();

        let response = ListTasksResponse {
            count: task_summaries.len(),
            tasks: task_summaries,
            project_id: project_id.to_string(),
            applied_filters: ListTasksFilters {
                status: status.clone(),
                limit: task_limit as i32,
            },
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Start task execution with specified executor.")]
    async fn start_task_attempt(
        &self,
        Parameters(StartTaskAttemptRequest {
            task_id,
            executor,
            variant,
            base_branch,
        }): Parameters<StartTaskAttemptRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let base_branch = base_branch.trim().to_string();
        if base_branch.is_empty() {
            return Self::err("Base branch must not be empty.".to_string(), None::<String>);
        }

        let executor_trimmed = executor.trim();
        if executor_trimmed.is_empty() {
            return Self::err("Executor must not be empty.".to_string(), None::<String>);
        }

        let normalized_executor = executor_trimmed.replace('-', "_").to_ascii_uppercase();
        let base_executor = match BaseCodingAgent::from_str(&normalized_executor) {
            Ok(exec) => exec,
            Err(_) => {
                return Self::err(
                    format!("Unknown executor '{executor_trimmed}'."),
                    None::<String>,
                );
            }
        };

        let variant = variant.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let executor_profile_id = ExecutorProfileId {
            executor: base_executor,
            variant,
        };

        let payload = CreateTaskAttemptBody {
            task_id,
            executor_profile_id,
            base_branch,
            target_node_id: None,
            use_parent_worktree: None,
        };

        let url = self.url("/api/task-attempts");
        let attempt: TaskAttempt = match self.send_json(self.client.post(&url).json(&payload)).await
        {
            Ok(attempt) => attempt,
            Err(e) => return Ok(e),
        };

        let response = StartTaskAttemptResponse {
            task_id: attempt.task_id.to_string(),
            attempt_id: attempt.id.to_string(),
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Update task title/description/status.")]
    async fn update_task(
        &self,
        Parameters(UpdateTaskRequest {
            task_id,
            title,
            description,
            status,
        }): Parameters<UpdateTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let status = if let Some(ref status_str) = status {
            match TaskStatus::from_str(status_str) {
                Ok(s) => Some(s),
                Err(_) => {
                    return Self::err(
                        "Invalid status filter. Valid values: 'todo', 'in-progress', 'in-review', 'done', 'cancelled'".to_string(),
                        Some(status_str.to_string()),
                    );
                }
            }
        } else {
            None
        };

        let payload = UpdateTask {
            title,
            description,
            status,
            parent_task_id: None,
            image_ids: None,
        };
        let url = self.url(&format!("/api/tasks/{}", task_id));
        let updated_task: Task = match self.send_json(self.client.put(&url).json(&payload)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        let details = TaskDetails::from_task(updated_task);
        let repsonse = UpdateTaskResponse { task: details };
        TaskServer::success(&repsonse)
    }

    #[tool(description = "Delete a task.")]
    async fn delete_task(
        &self,
        Parameters(DeleteTaskRequest { task_id }): Parameters<DeleteTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}", task_id));
        if let Err(e) = self
            .send_json::<serde_json::Value>(self.client.delete(&url))
            .await
        {
            return Ok(e);
        }

        let repsonse = DeleteTaskResponse {
            deleted_task_id: Some(task_id.to_string()),
        };

        TaskServer::success(&repsonse)
    }

    #[tool(description = "Get task details.")]
    async fn get_task(
        &self,
        Parameters(GetTaskRequest { task_id }): Parameters<GetTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}", task_id));
        let task: Task = match self.send_json(self.client.get(&url)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        let details = TaskDetails::from_task(task);
        let response = GetTaskResponse { task: details };

        TaskServer::success(&response)
    }

    // ===== Task Variables MCP Tools =====

    #[tool(description = "Get resolved variables (includes inherited).")]
    async fn get_task_variables(
        &self,
        Parameters(GetTaskVariablesRequest { task_id }): Parameters<GetTaskVariablesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}/variables/resolved", task_id));
        let variables: Vec<ResolvedVariable> = match self.send_json(self.client.get(&url)).await {
            Ok(v) => v,
            Err(e) => return Ok(e),
        };

        let variable_details: Vec<TaskVariableDetails> = variables
            .into_iter()
            .map(TaskVariableDetails::from_resolved)
            .collect();

        let response = GetTaskVariablesResponse {
            task_id: task_id.to_string(),
            count: variable_details.len(),
            variables: variable_details,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Set/update variable. Names: UPPER_SNAKE_CASE.")]
    async fn set_task_variable(
        &self,
        Parameters(SetTaskVariableRequest {
            task_id,
            name,
            value,
        }): Parameters<SetTaskVariableRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // First, check if the variable already exists on this task
        let list_url = self.url(&format!("/api/tasks/{}/variables", task_id));
        let existing_vars: Vec<TaskVariable> =
            match self.send_json(self.client.get(&list_url)).await {
                Ok(v) => v,
                Err(e) => return Ok(e),
            };

        // Look for existing variable with same name
        let existing = existing_vars.iter().find(|v| v.name == name);

        let (variable, created) = if let Some(existing_var) = existing {
            // Update existing variable
            let update_url = self.url(&format!(
                "/api/tasks/{}/variables/{}",
                task_id, existing_var.id
            ));
            let payload = serde_json::json!({ "value": value });
            let updated: TaskVariable = match self
                .send_json(self.client.put(&update_url).json(&payload))
                .await
            {
                Ok(v) => v,
                Err(e) => return Ok(e),
            };
            (updated, false)
        } else {
            // Create new variable
            let create_url = self.url(&format!("/api/tasks/{}/variables", task_id));
            let payload = serde_json::json!({ "name": name, "value": value });
            let created: TaskVariable = match self
                .send_json(self.client.post(&create_url).json(&payload))
                .await
            {
                Ok(v) => v,
                Err(e) => return Ok(e),
            };
            (created, true)
        };

        let response = SetTaskVariableResponse {
            id: variable.id.to_string(),
            name: variable.name,
            value: variable.value,
            task_id: task_id.to_string(),
            created,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Delete a task variable.")]
    async fn delete_task_variable(
        &self,
        Parameters(DeleteTaskVariableRequest { task_id, name }): Parameters<
            DeleteTaskVariableRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        // First, get the list of variables on this task to find the ID
        let list_url = self.url(&format!("/api/tasks/{}/variables", task_id));
        let existing_vars: Vec<TaskVariable> =
            match self.send_json(self.client.get(&list_url)).await {
                Ok(v) => v,
                Err(e) => return Ok(e),
            };

        // Look for variable with matching name
        let variable = match existing_vars.iter().find(|v| v.name == name) {
            Some(v) => v,
            None => {
                return Self::err(
                    format!("Variable '{}' not found on task {}", name, task_id),
                    None::<String>,
                );
            }
        };

        // Delete the variable by ID
        let delete_url = self.url(&format!("/api/tasks/{}/variables/{}", task_id, variable.id));
        if let Err(e) = self
            .send_json::<serde_json::Value>(self.client.delete(&delete_url))
            .await
        {
            return Ok(e);
        }

        let response = DeleteTaskVariableResponse {
            deleted_variable_name: name,
            task_id: task_id.to_string(),
        };

        TaskServer::success(&response)
    }

    // ===== Task Attempt Execution Control MCP Tools =====

    #[tool(description = "Stop running task attempt.")]
    async fn stop_task_attempt(
        &self,
        Parameters(StopTaskAttemptRequest { attempt_id }): Parameters<StopTaskAttemptRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-attempts/{}/stop", attempt_id));

        // POST to stop endpoint - it returns () on success
        if let Err(e) = self
            .send_json::<serde_json::Value>(self.client.post(&url))
            .await
        {
            return Ok(e);
        }

        let response = StopTaskAttemptResponse {
            attempt_id: attempt_id.to_string(),
            stopped: true,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Get attempt status with execution details.")]
    async fn get_task_attempt_status(
        &self,
        Parameters(GetTaskAttemptStatusRequest { attempt_id }): Parameters<
            GetTaskAttemptStatusRequest,
        >,
    ) -> Result<CallToolResult, ErrorData> {
        // Fetch the task attempt
        let attempt_url = self.url(&format!("/api/task-attempts/{}", attempt_id));
        let attempt: TaskAttempt = match self.send_json(self.client.get(&attempt_url)).await {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };

        // Fetch execution processes for this attempt
        let processes_url = self.url(&format!(
            "/api/execution-processes?task_attempt_id={}",
            attempt_id
        ));
        let processes: Vec<ExecutionProcess> = self
            .send_json(self.client.get(&processes_url))
            .await
            .unwrap_or_default();

        let process_summaries: Vec<ExecutionProcessSummary> = processes
            .into_iter()
            .map(|p| ExecutionProcessSummary {
                id: p.id.to_string(),
                run_reason: format!("{:?}", p.run_reason).to_lowercase(),
                status: format!("{:?}", p.status).to_lowercase(),
                started_at: p.started_at.to_rfc3339(),
                completed_at: p.completed_at.map(|dt| dt.to_rfc3339()),
            })
            .collect();

        let response = TaskAttemptStatusResponse {
            attempt_id: attempt.id.to_string(),
            task_id: attempt.task_id.to_string(),
            branch: attempt.branch,
            executor: attempt.executor,
            worktree_deleted: attempt.worktree_deleted,
            created_at: attempt.created_at.to_rfc3339(),
            processes: process_summaries,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "List attempts for a task.")]
    async fn list_task_attempts(
        &self,
        Parameters(ListTaskAttemptsRequest { task_id, limit }): Parameters<ListTaskAttemptsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/task-attempts?task_id={}", task_id));
        let attempts: Vec<TaskAttempt> = match self.send_json(self.client.get(&url)).await {
            Ok(a) => a,
            Err(e) => return Ok(e),
        };

        let attempt_limit = limit.unwrap_or(50).max(0) as usize;
        let limited: Vec<TaskAttempt> = attempts.into_iter().take(attempt_limit).collect();

        let attempt_summaries: Vec<TaskAttemptSummary> = limited
            .into_iter()
            .map(|a| TaskAttemptSummary {
                id: a.id.to_string(),
                branch: a.branch,
                executor: a.executor,
                created_at: a.created_at.to_rfc3339(),
                worktree_deleted: a.worktree_deleted,
            })
            .collect();

        let response = ListTaskAttemptsResponse {
            task_id: task_id.to_string(),
            count: attempt_summaries.len(),
            attempts: attempt_summaries,
        };

        TaskServer::success(&response)
    }

    // ===== Label MCP Tools =====

    #[tool(description = "Get task labels.")]
    async fn get_task_labels(
        &self,
        Parameters(GetTaskLabelsRequest { task_id }): Parameters<GetTaskLabelsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}/labels", task_id));
        let labels: Vec<Label> = match self.send_json(self.client.get(&url)).await {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let label_summaries: Vec<LabelSummary> =
            labels.into_iter().map(LabelSummary::from_label).collect();

        let response = GetTaskLabelsResponse {
            task_id: task_id.to_string(),
            count: label_summaries.len(),
            labels: label_summaries,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "Set task labels. Empty array clears all.")]
    async fn set_task_labels(
        &self,
        Parameters(SetTaskLabelsRequest { task_id, label_ids }): Parameters<SetTaskLabelsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}/labels", task_id));
        let payload = serde_json::json!({ "label_ids": label_ids });

        let labels: Vec<Label> = match self.send_json(self.client.put(&url).json(&payload)).await {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let label_summaries: Vec<LabelSummary> =
            labels.into_iter().map(LabelSummary::from_label).collect();

        let response = SetTaskLabelsResponse {
            task_id: task_id.to_string(),
            count: label_summaries.len(),
            labels: label_summaries,
        };

        TaskServer::success(&response)
    }

    #[tool(description = "List available labels.")]
    async fn list_labels(
        &self,
        Parameters(ListLabelsRequest { project_id }): Parameters<ListLabelsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = if let Some(pid) = project_id {
            self.url(&format!("/api/labels?project_id={}", pid))
        } else {
            self.url("/api/labels")
        };

        let labels: Vec<Label> = match self.send_json(self.client.get(&url)).await {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let label_summaries: Vec<LabelSummary> =
            labels.into_iter().map(LabelSummary::from_label).collect();

        let response = ListLabelsResponse {
            count: label_summaries.len(),
            labels: label_summaries,
            project_id: project_id.map(|p| p.to_string()),
        };

        TaskServer::success(&response)
    }

    // ===== Nodes MCP Tools =====

    #[tool(description = "List swarm nodes for task's project.")]
    async fn list_nodes(
        &self,
        Parameters(ListNodesRequest { task_id }): Parameters<ListNodesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}/available-nodes", task_id));

        let response: ListProjectNodesResponse = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let node_summaries: Vec<NodeSummary> = response
            .nodes
            .into_iter()
            .map(NodeSummary::from_project_node_info)
            .collect();

        let result = ListNodesResponse {
            task_id: task_id.to_string(),
            count: node_summaries.len(),
            nodes: node_summaries,
        };

        TaskServer::success(&result)
    }
}

#[tool_handler]
impl ServerHandler for TaskServer {
    fn get_info(&self) -> ServerInfo {
        let instruction = "Use 'get_context' with your working directory (cwd) to fetch project/task/attempt metadata for the active Vibe Kanban attempt. A task and project management server. If you need to create or update tickets or tasks then use these tools. Most of them absolutely require that you pass the `project_id` of the project that you are currently working on. You can get project ids by using `list projects`. Call `list_tasks` to fetch the `task_ids` of all the tasks in a project`.. TOOLS: 'list_projects', 'list_tasks', 'create_task', 'start_workspace_session', 'get_task', 'update_task', 'delete_task', 'list_repos'. Make sure to pass `project_id` or `task_id` where required. You can use list tools to get the available ids. Task variables: Use 'get_task_variables', 'set_task_variable', and 'delete_task_variable' to manage variables that are expanded in task descriptions using $VAR or ${VAR} syntax. Task attempts: Use 'stop_task_attempt', 'get_task_attempt_status', and 'list_task_attempts' to control and monitor task execution. Labels: Use 'get_task_labels', 'set_task_labels', and 'list_labels' to manage task labels for categorization. Nodes: Use 'list_nodes' to find swarm nodes available for a task's project.".to_string();

        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "vibe-kanban".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some(instruction),
        }
    }
}
