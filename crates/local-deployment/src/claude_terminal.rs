use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use executors::{
    command::{CommandBuilder, apply_overrides},
    env::ExecutionEnv,
    executors::claude_terminal::{ClaudeTerminal, ClaudeTerminalSettingsMergeMode},
};
use serde_json::{Value, json};
use tokio::{fs, process::Command};
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
    pub env: BTreeMap<String, String>,
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
    ];
    for (key, value) in args.env {
        command_args.extend(["-e".to_string(), format!("{key}={value}")]);
    }
    command_args.push(args.claude_program);
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

pub fn build_tmux_kill_session_command(session_name: String) -> ShellCommand {
    ShellCommand {
        program: "tmux".to_string(),
        args: vec!["kill-session".to_string(), "-t".to_string(), session_name],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxStartDecision {
    AttachExistingTmux,
    ResumeClaudeSession(String),
    StartFresh,
}

pub fn decide_tmux_start(
    tmux_session_exists: bool,
    saved_claude_session_id: Option<String>,
) -> TmuxStartDecision {
    if tmux_session_exists {
        TmuxStartDecision::AttachExistingTmux
    } else if let Some(session_id) = saved_claude_session_id {
        TmuxStartDecision::ResumeClaudeSession(session_id)
    } else {
        TmuxStartDecision::StartFresh
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeTerminalStartRequest {
    pub execution_id: Uuid,
    pub working_dir: PathBuf,
    pub prompt: String,
    pub resume_session_id: Option<String>,
    pub executor: ClaudeTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeTerminalStartResult {
    pub session_name: String,
    pub settings_path: PathBuf,
    pub decision: TmuxStartDecision,
}

pub async fn start_or_resume(
    request: ClaudeTerminalStartRequest,
    env: &ExecutionEnv,
) -> anyhow::Result<ClaudeTerminalStartResult> {
    let session_name = tmux_session_name(request.execution_id);
    let settings_path = claude_terminal_settings_path(&request.working_dir, request.execution_id);
    let command_env = env.clone().with_profile(&request.executor.cmd);
    let exists = tmux_session_exists(&session_name, &command_env).await?;
    let decision = decide_tmux_start(exists, request.resume_session_id.clone());

    if matches!(decision, TmuxStartDecision::AttachExistingTmux) {
        return Ok(ClaudeTerminalStartResult {
            session_name,
            settings_path,
            decision,
        });
    }

    write_settings_file(
        &settings_path,
        request.execution_id,
        request
            .executor
            .settings_merge_mode
            .unwrap_or(ClaudeTerminalSettingsMergeMode::MergeExisting),
    )
    .await?;

    let resume_session_id = match &decision {
        TmuxStartDecision::ResumeClaudeSession(session_id) => Some(session_id.as_str()),
        TmuxStartDecision::AttachExistingTmux | TmuxStartDecision::StartFresh => None,
    };
    let claude = request
        .executor
        .build_cli_args(settings_path.to_string_lossy(), resume_session_id);
    let claude = build_claude_shell_command(&request.executor, claude.args).await?;
    let command = build_tmux_new_session_command(TmuxStartArgs {
        session_name: session_name.clone(),
        working_dir: request.working_dir,
        env: command_env
            .vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        claude_program: claude.program,
        claude_args: claude.args,
        initial_prompt: Some(request.prompt),
    });
    run_shell_command(&command, &command_env).await?;

    Ok(ClaudeTerminalStartResult {
        session_name,
        settings_path,
        decision,
    })
}

pub fn claude_terminal_settings_path(working_dir: &Path, execution_id: Uuid) -> PathBuf {
    let _ = working_dir;
    utils::assets::asset_dir()
        .join("claude-terminal")
        .join("settings")
        .join(format!("{execution_id}.settings.json"))
}

pub fn claude_terminal_transcript_state_path(execution_id: Uuid) -> PathBuf {
    utils::assets::asset_dir()
        .join(".vibe-kanban")
        .join("claude-terminal")
        .join("transcripts")
        .join(format!("{execution_id}.json"))
}

pub fn hook_base_url_from_env() -> String {
    if let Ok(base) = std::env::var("VIBE_BACKEND_URL")
        && !base.trim().is_empty()
    {
        return format!("{}/api/claude-hooks", base.trim_end_matches('/'));
    }

    let port = std::env::var("BACKEND_PORT")
        .or_else(|_| std::env::var("PORT"))
        .unwrap_or_else(|_| "3001".to_string());
    format!("http://127.0.0.1:{port}/api/claude-hooks")
}

async fn build_claude_shell_command(
    executor: &ClaudeTerminal,
    args: Vec<String>,
) -> anyhow::Result<ShellCommand> {
    let builder = CommandBuilder::new("claude").params(args);
    let builder = apply_overrides(builder, &executor.cmd)?;
    let (program, args) = builder.build_initial()?.into_resolved().await?;
    Ok(ShellCommand {
        program: program.to_string_lossy().to_string(),
        args,
    })
}

async fn write_settings_file(
    settings_path: &Path,
    execution_id: Uuid,
    merge_mode: ClaudeTerminalSettingsMergeMode,
) -> anyhow::Result<()> {
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let settings = build_claude_settings_json(
        execution_id,
        &hook_base_url_from_env(),
        match merge_mode {
            ClaudeTerminalSettingsMergeMode::MergeExisting => SettingsMergeMode::MergeExisting,
            ClaudeTerminalSettingsMergeMode::VibeOnly => SettingsMergeMode::VibeOnly,
        },
    );
    fs::write(settings_path, serde_json::to_vec_pretty(&settings)?).await?;
    Ok(())
}

pub async fn tmux_session_exists(session_name: &str, env: &ExecutionEnv) -> anyhow::Result<bool> {
    let command = ShellCommand {
        program: "tmux".to_string(),
        args: vec![
            "has-session".to_string(),
            "-t".to_string(),
            session_name.to_string(),
        ],
    };
    match run_shell_command(&command, env).await {
        Ok(()) => Ok(true),
        Err(err) if is_tmux_missing_session_error(&err) => Ok(false),
        Err(err) => Err(err),
    }
}

async fn run_shell_command(command: &ShellCommand, env: &ExecutionEnv) -> anyhow::Result<()> {
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    env.apply_to_command(&mut process);
    let output = process.output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
        "{} exited with status {:?}: {}",
        command.program,
        output.status.code(),
        stderr.trim()
    );
}

pub async fn kill_tmux_session(execution_id: Uuid, env: &ExecutionEnv) -> anyhow::Result<()> {
    let command = build_tmux_kill_session_command(tmux_session_name(execution_id));
    run_shell_command(&command, env).await
}

pub async fn kill_tmux_session_if_exists(
    execution_id: Uuid,
    env: &ExecutionEnv,
) -> anyhow::Result<bool> {
    match kill_tmux_session(execution_id, env).await {
        Ok(()) => Ok(true),
        Err(err) if is_tmux_missing_session_error(&err) => Ok(false),
        Err(err) => Err(err),
    }
}

fn is_tmux_missing_session_error(err: &anyhow::Error) -> bool {
    let message = err.to_string();
    message.contains("can't find session")
        || message.contains("no server running")
        || message.contains("failed to connect to server")
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
            env: BTreeMap::new(),
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
            env: BTreeMap::from([("FOO".to_string(), "bar".to_string())]),
            claude_program: "claude".to_string(),
            claude_args: vec!["--settings".to_string(), "/tmp/settings.json".to_string()],
            initial_prompt: Some("-p".to_string()),
        });

        assert!(
            command
                .args
                .windows(2)
                .any(|pair| { pair == ["-e".to_string(), "FOO=bar".to_string()] })
        );
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

    #[test]
    fn kill_command_targets_execution_tmux_session() {
        let execution_id = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        let command = build_tmux_kill_session_command(tmux_session_name(execution_id));

        assert_eq!(command.program, "tmux");
        assert_eq!(
            command.args,
            vec![
                "kill-session".to_string(),
                "-t".to_string(),
                "vk-claude-44444444-4444-4444-4444-444444444444".to_string(),
            ]
        );
    }

    #[test]
    fn tmux_missing_session_errors_are_specific() {
        assert!(is_tmux_missing_session_error(&anyhow::anyhow!(
            "tmux exited with status Some(1): can't find session: vk-claude-test"
        )));
        assert!(is_tmux_missing_session_error(&anyhow::anyhow!(
            "tmux exited with status Some(1): no server running on /tmp/tmux/default"
        )));
        assert!(!is_tmux_missing_session_error(&anyhow::anyhow!(
            "tmux exited with status Some(1): permission denied"
        )));
    }

    #[test]
    fn start_decision_attaches_existing_tmux_session_before_new_launch() {
        let decision = decide_tmux_start(false, Some("claude-session-123".to_string()));
        assert_eq!(
            decision,
            TmuxStartDecision::ResumeClaudeSession("claude-session-123".to_string())
        );

        let decision = decide_tmux_start(true, Some("claude-session-123".to_string()));
        assert_eq!(decision, TmuxStartDecision::AttachExistingTmux);

        let decision = decide_tmux_start(false, None);
        assert_eq!(decision, TmuxStartDecision::StartFresh);
    }
}
