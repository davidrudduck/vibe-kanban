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
}
