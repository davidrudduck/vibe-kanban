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
        StandardCodingAgentExecutor, claude::ClaudeEffort, claude::types::PermissionMode,
    },
    model_selector::PermissionPolicy,
    profile::ExecutorConfig,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
pub struct ClaudeTerminal {
    #[serde(default)]
    pub append_prompt: AppendPrompt,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approvals: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<ClaudeEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dangerously_skip_permissions: Option<bool>,
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
            plan: None,
            approvals: None,
            model: None,
            effort: None,
            agent: None,
            dangerously_skip_permissions: None,
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

        if self.dangerously_skip_permissions.unwrap_or(false) {
            args.push("--dangerously-skip-permissions".to_string());
        } else if self.plan.unwrap_or(false) {
            args.push(format!("--permission-mode={}", PermissionMode::Plan));
        } else if self.approvals.unwrap_or(false) {
            args.push(format!("--permission-mode={}", PermissionMode::Default));
        }

        if let Some(model) = &self.model {
            args.extend(["--model".to_string(), model.clone()]);
        }

        if let Some(effort) = &self.effort {
            args.extend(["--effort".to_string(), effort.as_ref().to_string()]);
        }

        if let Some(agent) = &self.agent {
            args.extend(["--agent".to_string(), agent.clone()]);
        }

        if let Some(session_id) = resume_session_id {
            args.extend(["--resume".to_string(), session_id.to_string()]);
        }

        if matches!(
            self.settings_merge_mode,
            Some(ClaudeTerminalSettingsMergeMode::VibeOnly)
        ) {
            args.extend(["--setting-sources".to_string(), String::new()]);
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
        if let Some(agent) = &executor_config.agent_id {
            self.agent = Some(agent.clone());
        }
        if let Some(reasoning_id) = &executor_config.reasoning_id {
            self.effort = reasoning_id.parse().ok();
        }
        if let Some(permission_policy) = executor_config.permission_policy.clone() {
            match permission_policy {
                PermissionPolicy::Plan => {
                    self.plan = Some(true);
                    self.approvals = Some(false);
                    self.dangerously_skip_permissions = Some(false);
                }
                PermissionPolicy::Supervised => {
                    self.plan = Some(false);
                    self.approvals = Some(true);
                    self.dangerously_skip_permissions = Some(false);
                }
                PermissionPolicy::Auto => {
                    self.plan = Some(false);
                    self.approvals = Some(false);
                    self.dangerously_skip_permissions = Some(true);
                }
            }
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
            agent_id: self.agent.clone(),
            reasoning_id: self.effort.as_ref().map(|e| e.as_ref().to_owned()),
            permission_policy: Some(if self.plan.unwrap_or(false) {
                PermissionPolicy::Plan
            } else if self.approvals.unwrap_or(false) {
                PermissionPolicy::Supervised
            } else {
                PermissionPolicy::Auto
            }),
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

    #[test]
    fn vibe_only_command_disables_file_based_setting_sources() {
        let executor = ClaudeTerminal {
            settings_merge_mode: Some(ClaudeTerminalSettingsMergeMode::VibeOnly),
            ..ClaudeTerminal::default()
        };
        let command = executor.build_cli_args("settings.json", None);

        assert!(
            command
                .args
                .windows(2)
                .any(|pair| { pair == ["--setting-sources".to_string(), String::new()] })
        );
    }

    #[test]
    fn command_includes_claude_cli_options_shared_with_sdk_executor() {
        let executor = ClaudeTerminal {
            plan: Some(true),
            approvals: Some(true),
            model: Some("claude-sonnet-4-5".to_string()),
            effort: Some(super::super::claude::ClaudeEffort::High),
            agent: Some("general-purpose".to_string()),
            dangerously_skip_permissions: Some(true),
            ..ClaudeTerminal::default()
        };
        let command = executor.build_cli_args("settings.json", None);

        assert!(
            command
                .args
                .contains(&"--dangerously-skip-permissions".to_string())
        );
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--model".to_string(), "claude-sonnet-4-5".to_string()])
        );
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--effort".to_string(), "high".to_string()])
        );
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--agent".to_string(), "general-purpose".to_string()])
        );
        assert!(
            !command
                .args
                .iter()
                .any(|arg| arg.starts_with("--permission-mode"))
        );
    }

    #[test]
    fn command_maps_plan_and_approvals_to_permission_mode_when_not_skipping_permissions() {
        let plan_executor = ClaudeTerminal {
            plan: Some(true),
            approvals: Some(true),
            ..ClaudeTerminal::default()
        };
        let plan_command = plan_executor.build_cli_args("settings.json", None);
        assert!(
            plan_command
                .args
                .contains(&"--permission-mode=plan".to_string())
        );

        let approvals_executor = ClaudeTerminal {
            approvals: Some(true),
            ..ClaudeTerminal::default()
        };
        let approvals_command = approvals_executor.build_cli_args("settings.json", None);
        assert!(
            approvals_command
                .args
                .contains(&"--permission-mode=default".to_string())
        );
    }

    #[test]
    fn executor_config_overrides_drive_terminal_cli_options() {
        let mut executor = ClaudeTerminal::default();
        executor.apply_overrides(&ExecutorConfig {
            executor: BaseCodingAgent::ClaudeTerminal,
            variant: None,
            model_id: Some("claude-sonnet-4-5".to_string()),
            agent_id: Some("reviewer".to_string()),
            reasoning_id: Some("high".to_string()),
            permission_policy: Some(PermissionPolicy::Plan),
        });

        let command = executor.build_cli_args("settings.json", None);
        assert!(command.args.contains(&"--permission-mode=plan".to_string()));
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--model".to_string(), "claude-sonnet-4-5".to_string()])
        );
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--effort".to_string(), "high".to_string()])
        );
        assert!(
            command
                .args
                .windows(2)
                .any(|pair| pair == ["--agent".to_string(), "reviewer".to_string()])
        );
    }
}
