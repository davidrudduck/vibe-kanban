use std::path::Path;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    command::CmdOverrides,
    env::ExecutionEnv,
    executors::{
        AppendPrompt, AvailabilityInfo, BaseCodingAgent, ExecutorError, SpawnedChild,
        StandardCodingAgentExecutor,
    },
    profile::ExecutorConfig,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
pub struct ClaudeTerminal {
    #[serde(default)]
    pub append_prompt: AppendPrompt,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_merge_mode: Option<ClaudeTerminalSettingsMergeMode>,
    #[serde(flatten)]
    pub cmd: CmdOverrides,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeTerminalSettingsMergeMode {
    MergeExisting,
    VibeOnly,
}

impl Default for ClaudeTerminal {
    fn default() -> Self {
        Self {
            append_prompt: AppendPrompt::default(),
            model: None,
            settings_merge_mode: Some(ClaudeTerminalSettingsMergeMode::MergeExisting),
            cmd: CmdOverrides::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeTerminalCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl ClaudeTerminal {
    pub fn build_cli_args(
        &self,
        settings_path: impl Into<String>,
        resume_session_id: Option<&str>,
    ) -> ClaudeTerminalCommand {
        let mut args = vec!["--settings".to_string(), settings_path.into()];

        if let Some(model) = &self.model {
            args.extend(["--model".to_string(), model.clone()]);
        }

        if let Some(session_id) = resume_session_id {
            args.extend(["--resume".to_string(), session_id.to_string()]);
        }

        ClaudeTerminalCommand {
            program: "claude".to_string(),
            args,
        }
    }
}

#[async_trait]
impl StandardCodingAgentExecutor for ClaudeTerminal {
    fn apply_overrides(&mut self, executor_config: &ExecutorConfig) {
        if let Some(model_id) = &executor_config.model_id {
            self.model = Some(model_id.clone());
        }
    }

    async fn spawn(
        &self,
        _current_dir: &Path,
        _prompt: &str,
        _env: &ExecutionEnv,
    ) -> Result<SpawnedChild, ExecutorError> {
        Err(ExecutorError::FollowUpNotSupported(
            "Claude Code Terminal is launched by the local tmux executor".to_string(),
        ))
    }

    async fn spawn_follow_up(
        &self,
        _current_dir: &Path,
        _prompt: &str,
        _session_id: &str,
        _reset_to_message_id: Option<&str>,
        _env: &ExecutionEnv,
    ) -> Result<SpawnedChild, ExecutorError> {
        Err(ExecutorError::FollowUpNotSupported(
            "Claude Code Terminal follow-up is launched by the local tmux executor".to_string(),
        ))
    }

    fn default_mcp_config_path(&self) -> Option<std::path::PathBuf> {
        dirs::home_dir().map(|home| home.join(".claude.json"))
    }

    fn get_availability_info(&self) -> AvailabilityInfo {
        let auth_file_path = dirs::home_dir().map(|home| home.join(".claude.json"));

        if let Some(path) = auth_file_path
            && let Some(timestamp) = std::fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
        {
            return AvailabilityInfo::LoginDetected {
                last_auth_timestamp: timestamp,
            };
        }

        AvailabilityInfo::NotFound
    }

    fn get_preset_options(&self) -> ExecutorConfig {
        ExecutorConfig {
            executor: BaseCodingAgent::ClaudeTerminal,
            variant: None,
            model_id: self.model.clone(),
            agent_id: None,
            reasoning_id: None,
            permission_policy: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_command_uses_interactive_claude_without_print_mode() {
        let executor = ClaudeTerminal::default();
        let command = executor.build_cli_args("settings.json", None);

        assert_eq!(command.program, "claude");
        assert!(command.args.contains(&"--settings".to_string()));
        assert!(command.args.contains(&"settings.json".to_string()));
        assert!(!command.args.contains(&"-p".to_string()));
        assert!(
            !command
                .args
                .contains(&"--output-format=stream-json".to_string())
        );
        assert!(
            !command
                .args
                .contains(&"--input-format=stream-json".to_string())
        );
    }

    #[test]
    fn resume_command_uses_claude_resume_session_id() {
        let executor = ClaudeTerminal::default();
        let command = executor.build_cli_args("settings.json", Some("abc-session"));

        assert!(
            command
                .args
                .windows(2)
                .any(|pair| { pair == ["--resume".to_string(), "abc-session".to_string()] })
        );
    }
}
