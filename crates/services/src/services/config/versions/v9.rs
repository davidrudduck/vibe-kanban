use anyhow::Error;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use v8::{
    DevBannerConfig, EditorConfig, EditorType, GitHubConfig, NotificationConfig, SoundFile,
    ThemeMode, UiLanguage,
};

use crate::services::config::versions::v8;

fn default_git_branch_prefix() -> String {
    "vk".to_string()
}

/// Default initial load limit for paginated logs
fn default_initial_load() -> i64 {
    100
}

/// Default max limit for paginated log requests
fn default_max_limit() -> i64 {
    500
}

/// Configuration for log pagination behavior
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PaginationConfig {
    /// Number of log entries to load initially (default: 100)
    #[serde(default = "default_initial_load")]
    pub initial_load: i64,
    /// Maximum entries per page request (default: 500)
    #[serde(default = "default_max_limit")]
    pub max_limit: i64,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            initial_load: default_initial_load(),
            max_limit: default_max_limit(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Config {
    pub config_version: String,
    pub theme: ThemeMode,
    pub executor_profile: ExecutorProfileId,
    pub disclaimer_acknowledged: bool,
    pub onboarding_acknowledged: bool,
    pub notifications: NotificationConfig,
    pub editor: EditorConfig,
    pub github: GitHubConfig,
    pub analytics_enabled: bool,
    #[serde(default)]
    pub sentry_enabled: bool,
    pub workspace_dir: Option<String>,
    pub last_app_version: Option<String>,
    pub show_release_notes: bool,
    #[serde(default)]
    pub language: UiLanguage,
    #[serde(default = "default_git_branch_prefix")]
    pub git_branch_prefix: String,
    #[serde(default)]
    pub dev_banner: DevBannerConfig,
    /// Pagination settings for log display
    #[serde(default)]
    pub pagination: PaginationConfig,
}

impl Config {
    fn from_v8_config(old_config: v8::Config) -> Self {
        Self {
            config_version: "v9".to_string(),
            theme: old_config.theme,
            executor_profile: old_config.executor_profile,
            disclaimer_acknowledged: old_config.disclaimer_acknowledged,
            onboarding_acknowledged: old_config.onboarding_acknowledged,
            notifications: old_config.notifications,
            editor: old_config.editor,
            github: old_config.github,
            analytics_enabled: old_config.analytics_enabled,
            sentry_enabled: old_config.sentry_enabled,
            workspace_dir: old_config.workspace_dir,
            last_app_version: old_config.last_app_version,
            show_release_notes: old_config.show_release_notes,
            language: old_config.language,
            git_branch_prefix: old_config.git_branch_prefix,
            dev_banner: old_config.dev_banner,
            pagination: PaginationConfig::default(),
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v8::Config::from(raw_config.to_string());
        Ok(Self::from_v8_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v9"
        {
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(config) => {
                tracing::info!("Config upgraded to v9");
                config
            }
            Err(e) => {
                tracing::warn!("Config migration failed: {}, using default", e);
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: "v9".to_string(),
            theme: ThemeMode::System,
            executor_profile: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
            disclaimer_acknowledged: false,
            onboarding_acknowledged: false,
            notifications: NotificationConfig::default(),
            editor: EditorConfig::default(),
            github: GitHubConfig::default(),
            analytics_enabled: true,
            sentry_enabled: false,
            workspace_dir: None,
            last_app_version: None,
            show_release_notes: false,
            language: UiLanguage::default(),
            git_branch_prefix: default_git_branch_prefix(),
            dev_banner: DevBannerConfig::default(),
            pagination: PaginationConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v8_migrates_to_v9_with_default_pagination() {
        let v8_json = r#"{
            "config_version": "v8",
            "theme": "SYSTEM",
            "executor_profile": {"executor": "CLAUDE_CODE"},
            "disclaimer_acknowledged": true,
            "onboarding_acknowledged": true,
            "notifications": {
                "sound_enabled": true,
                "push_enabled": true,
                "sound_file": "ABSTRACT_SOUND1"
            },
            "editor": {
                "enabled": true,
                "editor_type": "VS_CODE"
            },
            "github": {
                "oauth_token": null
            },
            "analytics_enabled": true,
            "sentry_enabled": false,
            "workspace_dir": null,
            "last_app_version": "1.0.0",
            "show_release_notes": true,
            "language": "EN",
            "git_branch_prefix": "vk",
            "dev_banner": {}
        }"#;

        // First, verify that v8::Config can parse this
        let v8_result = serde_json::from_str::<v8::Config>(v8_json);
        if let Err(e) = &v8_result {
            panic!("v8 parse failed: {}", e);
        }
        let v8_config = v8_result.unwrap();
        assert_eq!(v8_config.config_version, "v8");
        assert!(v8_config.disclaimer_acknowledged);

        let config = Config::from(v8_json.to_string());

        assert_eq!(config.config_version, "v9");
        assert_eq!(config.pagination.initial_load, 100);
        assert_eq!(config.pagination.max_limit, 500);
        // Verify other fields migrated correctly
        assert!(config.disclaimer_acknowledged);
        assert!(config.onboarding_acknowledged);
        assert!(config.analytics_enabled);
    }

    #[test]
    fn test_v9_config_preserves_pagination_settings() {
        let v9_json = r#"{
            "config_version": "v9",
            "theme": "DARK",
            "executor_profile": {"executor": "CLAUDE_CODE"},
            "disclaimer_acknowledged": true,
            "onboarding_acknowledged": true,
            "notifications": {
                "sound_enabled": true,
                "push_enabled": true,
                "sound_file": "ABSTRACT_SOUND1"
            },
            "editor": {
                "enabled": true,
                "editor_type": "VS_CODE"
            },
            "github": {
                "oauth_token": null
            },
            "analytics_enabled": true,
            "sentry_enabled": false,
            "workspace_dir": null,
            "last_app_version": null,
            "show_release_notes": false,
            "language": "EN",
            "git_branch_prefix": "vk",
            "dev_banner": {},
            "pagination": {
                "initial_load": 200,
                "max_limit": 1000
            }
        }"#;

        let config = Config::from(v9_json.to_string());

        assert_eq!(config.config_version, "v9");
        assert_eq!(config.pagination.initial_load, 200);
        assert_eq!(config.pagination.max_limit, 1000);
    }

    #[test]
    fn test_default_pagination_config() {
        let config = PaginationConfig::default();
        assert_eq!(config.initial_load, 100);
        assert_eq!(config.max_limit, 500);
    }

    #[test]
    fn test_default_config_has_v9_version() {
        let config = Config::default();
        assert_eq!(config.config_version, "v9");
        assert_eq!(config.pagination.initial_load, 100);
        assert_eq!(config.pagination.max_limit, 500);
    }

    #[test]
    fn test_v9_config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.config_version, "v9");
        assert_eq!(parsed.pagination.initial_load, 100);
        assert_eq!(parsed.pagination.max_limit, 500);
    }

    #[test]
    fn test_migration_chain_from_older_versions() {
        // Test that old v7 configs still work (will go through v7 -> v8 -> v9)
        let v7_json = r#"{
            "config_version": "v7",
            "theme": "LIGHT",
            "executor_profile": {"executor": "CLAUDE_CODE"},
            "disclaimer_acknowledged": true,
            "onboarding_acknowledged": true,
            "github_login_acknowledged": true,
            "login_acknowledged": false,
            "telemetry_acknowledged": true,
            "notifications": {
                "sound_enabled": true,
                "push_enabled": true,
                "sound_file": "ABSTRACT_SOUND1"
            },
            "editor": {
                "enabled": true,
                "editor_type": "VS_CODE"
            },
            "github": {
                "oauth_token": null
            },
            "analytics_enabled": true,
            "workspace_dir": null,
            "last_app_version": null,
            "show_release_notes": false,
            "language": "EN",
            "git_branch_prefix": "vk"
        }"#;

        let config = Config::from(v7_json.to_string());

        assert_eq!(config.config_version, "v9");
        assert_eq!(config.pagination.initial_load, 100);
        assert_eq!(config.pagination.max_limit, 500);
    }
}
