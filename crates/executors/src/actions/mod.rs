use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    actions::{
        coding_agent_follow_up::CodingAgentFollowUpRequest,
        coding_agent_initial::CodingAgentInitialRequest, review::ReviewRequest,
        script::ScriptRequest,
    },
    approvals::ExecutorApprovalService,
    env::ExecutionEnv,
    executors::{BaseCodingAgent, ExecutorError, SpawnedChild},
};
pub mod coding_agent_follow_up;
pub mod coding_agent_initial;
pub mod review;
pub mod script;

pub use review::RepoReviewContext;

pub fn resolve_relative_working_dir(
    current_dir: &Path,
    working_dir: Option<&str>,
) -> Result<PathBuf, ExecutorError> {
    let Some(working_dir) = working_dir else {
        return Ok(current_dir.to_path_buf());
    };

    if working_dir.is_empty() {
        return Ok(current_dir.to_path_buf());
    }

    let relative_path = Path::new(working_dir);
    if relative_path.is_absolute() {
        return Err(ExecutorError::InvalidWorkingDir(
            "absolute paths are not allowed".to_string(),
        ));
    }

    for component in relative_path.components() {
        match component {
            Component::ParentDir => {
                return Err(ExecutorError::InvalidWorkingDir(
                    "parent directory traversal is not allowed".to_string(),
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(ExecutorError::InvalidWorkingDir(
                    "rooted paths are not allowed".to_string(),
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let effective_dir = current_dir.join(relative_path);
    if !effective_dir.starts_with(current_dir) {
        return Err(ExecutorError::InvalidWorkingDir(
            "effective path escapes the current workspace".to_string(),
        ));
    }

    Ok(effective_dir)
}

#[enum_dispatch]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "type")]
pub enum ExecutorActionType {
    CodingAgentInitialRequest,
    CodingAgentFollowUpRequest,
    ScriptRequest,
    ReviewRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExecutorAction {
    pub typ: ExecutorActionType,
    pub next_action: Option<Box<ExecutorAction>>,
}

impl ExecutorAction {
    pub fn new(typ: ExecutorActionType, next_action: Option<Box<ExecutorAction>>) -> Self {
        Self { typ, next_action }
    }
    pub fn append_action(mut self, action: ExecutorAction) -> Self {
        if let Some(next) = self.next_action {
            self.next_action = Some(Box::new(next.append_action(action)));
        } else {
            self.next_action = Some(Box::new(action));
        }
        self
    }

    pub fn typ(&self) -> &ExecutorActionType {
        &self.typ
    }

    pub fn next_action(&self) -> Option<&ExecutorAction> {
        self.next_action.as_deref()
    }

    pub fn base_executor(&self) -> Option<BaseCodingAgent> {
        match self.typ() {
            ExecutorActionType::CodingAgentInitialRequest(request) => Some(request.base_executor()),
            ExecutorActionType::CodingAgentFollowUpRequest(request) => {
                Some(request.base_executor())
            }
            ExecutorActionType::ReviewRequest(request) => Some(request.base_executor()),
            ExecutorActionType::ScriptRequest(_) => None,
        }
    }
}

#[async_trait]
#[enum_dispatch(ExecutorActionType)]
pub trait Executable {
    async fn spawn(
        &self,
        current_dir: &Path,
        approvals: Arc<dyn ExecutorApprovalService>,
        env: &ExecutionEnv,
    ) -> Result<SpawnedChild, ExecutorError>;
}

#[async_trait]
impl Executable for ExecutorAction {
    async fn spawn(
        &self,
        current_dir: &Path,
        approvals: Arc<dyn ExecutorApprovalService>,
        env: &ExecutionEnv,
    ) -> Result<SpawnedChild, ExecutorError> {
        self.typ.spawn(current_dir, approvals, env).await
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::resolve_relative_working_dir;

    #[test]
    fn working_dir_rejects_absolute_paths() {
        let result = resolve_relative_working_dir(Path::new("/workspace"), Some("/tmp"));

        assert!(result.is_err());
    }

    #[test]
    fn working_dir_rejects_parent_traversal() {
        let result = resolve_relative_working_dir(Path::new("/workspace"), Some("../outside"));

        assert!(result.is_err());
    }

    #[test]
    fn working_dir_allows_normal_relative_paths() {
        let result = resolve_relative_working_dir(Path::new("/workspace"), Some("repo/src"))
            .expect("relative path should be accepted");

        assert_eq!(result, Path::new("/workspace/repo/src"));
    }

    #[test]
    fn working_dir_defaults_to_current_dir() {
        let result = resolve_relative_working_dir(Path::new("/workspace"), None)
            .expect("missing path should use current dir");

        assert_eq!(result, Path::new("/workspace"));
    }
}
