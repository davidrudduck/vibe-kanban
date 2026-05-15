use std::path::PathBuf;

use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMergeMode {
    MergeExisting,
    VibeOnly,
}

pub fn build_claude_settings_json(
    execution_id: Uuid,
    hook_base_url: &str,
    merge_mode: SettingsMergeMode,
) -> Value {
    let base = hook_base_url.trim_end_matches('/');
    let hook_url = |event: &str| format!("{base}/{execution_id}/{event}");

    let settings = json!({
        "allowedHttpHookUrls": [
            format!("{base}/*")
        ],
        "hooks": {
            "SessionStart": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("session-start") }
                    ]
                }
            ],
            "UserPromptSubmit": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("user-prompt-submit") }
                    ]
                }
            ],
            "PostToolUse": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("post-tool-use") }
                    ]
                }
            ],
            "PostToolUseFailure": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("post-tool-use-failure") }
                    ]
                }
            ],
            "Stop": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("stop") }
                    ]
                }
            ],
            "StopFailure": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("stop-failure") }
                    ]
                }
            ],
            "SessionEnd": [
                {
                    "hooks": [
                        { "type": "http", "url": hook_url("session-end") }
                    ]
                }
            ]
        }
    });

    let _ = merge_mode;

    settings
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TmuxStartArgs {
    pub session_name: String,
    pub working_dir: PathBuf,
    pub claude_program: String,
    pub claude_args: Vec<String>,
    pub initial_prompt: Option<String>,
}

pub fn tmux_session_name(execution_id: Uuid) -> String {
    format!("vk-claude-{execution_id}")
}

pub fn build_tmux_new_session_command(args: TmuxStartArgs) -> ShellCommand {
    let mut command_args = vec![
        "new-session".to_string(),
        "-d".to_string(),
        "-s".to_string(),
        args.session_name,
        "-c".to_string(),
        args.working_dir.to_string_lossy().to_string(),
        args.claude_program,
    ];
    command_args.extend(args.claude_args);
    if let Some(prompt) = args.initial_prompt {
        command_args.push("--".to_string());
        command_args.push(prompt);
    }
    ShellCommand {
        program: "tmux".to_string(),
        args: command_args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn generated_settings_contains_only_vibe_hook_additions() {
        let execution_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let settings = build_claude_settings_json(
            execution_id,
            "http://127.0.0.1:9000/api/claude-hooks",
            SettingsMergeMode::MergeExisting,
        );

        assert!(settings.get("hooks").is_some());
        assert!(settings.get("allowedHttpHookUrls").is_some());
        assert!(settings.get("disableAllHooks").is_none());
        assert!(settings.get("mcpServers").is_none());
        assert!(settings.get("permissions").is_none());
    }

    #[test]
    fn generated_settings_includes_success_and_failure_tool_hooks() {
        let execution_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let settings = build_claude_settings_json(
            execution_id,
            "http://127.0.0.1:9000/api/claude-hooks",
            SettingsMergeMode::MergeExisting,
        );

        let hooks = settings
            .get("hooks")
            .and_then(|value| value.as_object())
            .unwrap();
        assert!(hooks.contains_key("PostToolUse"));
        assert!(hooks.contains_key("PostToolUseFailure"));
    }

    #[test]
    fn vibe_only_mode_does_not_emit_invalid_settings_source_key() {
        let execution_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let settings = build_claude_settings_json(
            execution_id,
            "http://127.0.0.1:9000/api/claude-hooks",
            SettingsMergeMode::VibeOnly,
        );

        assert!(settings.get("settingSources").is_none());
        assert!(settings.get("setting-sources").is_none());
        assert!(settings.get("disableAllHooks").is_none());
    }

    #[test]
    fn tmux_session_name_is_stable_and_prefixed() {
        let execution_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        assert_eq!(
            tmux_session_name(execution_id),
            "vk-claude-33333333-3333-3333-3333-333333333333"
        );
    }

    #[test]
    fn start_command_launches_claude_in_worktree_with_settings() {
        let command = build_tmux_new_session_command(TmuxStartArgs {
            session_name: "vk-claude-test".to_string(),
            working_dir: "/tmp/worktree".into(),
            claude_program: "claude".to_string(),
            claude_args: vec!["--settings".to_string(), "/tmp/settings.json".to_string()],
            initial_prompt: Some("fix the tests".to_string()),
        });

        assert_eq!(command.program, "tmux");
        assert_eq!(
            command.args,
            vec![
                "new-session".to_string(),
                "-d".to_string(),
                "-s".to_string(),
                "vk-claude-test".to_string(),
                "-c".to_string(),
                "/tmp/worktree".to_string(),
                "claude".to_string(),
                "--settings".to_string(),
                "/tmp/settings.json".to_string(),
                "--".to_string(),
                "fix the tests".to_string(),
            ]
        );
        assert!(!command.args.iter().any(|arg| arg == "-p"));
    }

    #[test]
    fn start_command_delimits_flag_like_prompt() {
        let command = build_tmux_new_session_command(TmuxStartArgs {
            session_name: "vk-claude-test".to_string(),
            working_dir: "/tmp/worktree".into(),
            claude_program: "claude".to_string(),
            claude_args: vec!["--settings".to_string(), "/tmp/settings.json".to_string()],
            initial_prompt: Some("-p".to_string()),
        });

        let delimiter_index = command
            .args
            .iter()
            .position(|arg| arg == "--")
            .expect("prompt delimiter missing");
        assert_eq!(
            command.args.get(delimiter_index + 1),
            Some(&"-p".to_string())
        );
    }
}
