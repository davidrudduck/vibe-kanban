use anyhow::Error;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use v9::{
    DevBannerConfig, EditorConfig, EditorType, GitHubConfig, NotificationConfig, PaginationConfig,
    SoundFile, ThemeMode, UiLanguage,
};

use crate::services::config::versions::v9;

fn default_git_branch_prefix() -> String {
    "vk".to_string()
}

/// Available fonts for UI elements (buttons, menus, navigation)
#[derive(Clone, Debug, Serialize, Deserialize, TS, Default, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum UiFont {
    #[default]
    Inter,
    Roboto,
    PublicSans,
    ChivoMono,
    System,
}

/// Available fonts for code blocks and monospace text
#[derive(Clone, Debug, Serialize, Deserialize, TS, Default, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum CodeFont {
    #[default]
    JetBrainsMono,
    CascadiaMono,
    Hack,
    IbmPlexMono,
    ChivoMono,
    System,
}

/// Available fonts for prose/reading content (task descriptions, markdown)
#[derive(Clone, Debug, Serialize, Deserialize, TS, Default, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum ProseFont {
    #[default]
    Inter,
    Roboto,
    Georgia,
    ChivoMono,
    System,
}

/// Font configuration for different text contexts
#[derive(Clone, Debug, Serialize, Deserialize, TS, Default, PartialEq)]
#[ts(export)]
pub struct FontConfig {
    /// Font for UI elements (buttons, menus, navigation)
    #[serde(default)]
    pub ui_font: UiFont,
    /// Font for code blocks and monospace text
    #[serde(default)]
    pub code_font: CodeFont,
    /// Font for prose/reading content (task descriptions, markdown)
    #[serde(default)]
    pub prose_font: ProseFont,
    /// Disable font ligatures in code contexts
    #[serde(default)]
    pub disable_ligatures: bool,
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
    /// Deprecated: analytics has been removed. Field kept for config compatibility.
    #[serde(default)]
    pub analytics_enabled: bool,
    /// Deprecated: Sentry error reporting has been removed. Field kept for config compatibility.
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
    /// Font settings for UI, code, and prose contexts
    #[serde(default)]
    pub fonts: FontConfig,
}

impl Config {
    fn from_v9_config(old_config: v9::Config) -> Self {
        Self {
            config_version: "v10".to_string(),
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
            pagination: old_config.pagination,
            fonts: FontConfig::default(),
        }
    }

    pub fn from_previous_version(raw_config: &str) -> Result<Self, Error> {
        let old_config = v9::Config::from(raw_config.to_string());
        Ok(Self::from_v9_config(old_config))
    }
}

impl From<String> for Config {
    fn from(raw_config: String) -> Self {
        if let Ok(config) = serde_json::from_str::<Config>(&raw_config)
            && config.config_version == "v10"
        {
            return config;
        }

        match Self::from_previous_version(&raw_config) {
            Ok(config) => {
                tracing::info!("Config upgraded to v10");
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
            config_version: "v10".to_string(),
            theme: ThemeMode::System,
            executor_profile: ExecutorProfileId::new(BaseCodingAgent::ClaudeCode),
            disclaimer_acknowledged: false,
            onboarding_acknowledged: false,
            notifications: NotificationConfig::default(),
            editor: EditorConfig::default(),
            github: GitHubConfig::default(),
            analytics_enabled: false, // Deprecated: analytics has been removed
            sentry_enabled: false,    // Deprecated: Sentry has been removed
            workspace_dir: None,
            last_app_version: None,
            show_release_notes: false,
            language: UiLanguage::default(),
            git_branch_prefix: default_git_branch_prefix(),
            dev_banner: DevBannerConfig::default(),
            pagination: PaginationConfig::default(),
            fonts: FontConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_font_config() {
        let config = FontConfig::default();
        assert_eq!(config.ui_font, UiFont::Inter);
        assert_eq!(config.code_font, CodeFont::JetBrainsMono);
        assert_eq!(config.prose_font, ProseFont::Inter);
        assert!(!config.disable_ligatures);
    }

    #[test]
    fn test_default_config_has_v10_version() {
        let config = Config::default();
        assert_eq!(config.config_version, "v10");
        assert_eq!(config.fonts.ui_font, UiFont::Inter);
        assert_eq!(config.fonts.code_font, CodeFont::JetBrainsMono);
        assert_eq!(config.fonts.prose_font, ProseFont::Inter);
    }

    #[test]
    fn test_v9_migrates_to_v10_with_default_fonts() {
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
            "last_app_version": "1.0.0",
            "show_release_notes": true,
            "language": "EN",
            "git_branch_prefix": "vk",
            "dev_banner": {},
            "pagination": {
                "initial_load": 200,
                "max_limit": 1000
            }
        }"#;

        let config = Config::from(v9_json.to_string());

        assert_eq!(config.config_version, "v10");
        assert_eq!(config.fonts.ui_font, UiFont::Inter);
        assert_eq!(config.fonts.code_font, CodeFont::JetBrainsMono);
        assert_eq!(config.fonts.prose_font, ProseFont::Inter);
        assert!(!config.fonts.disable_ligatures);
        // Verify other fields migrated correctly
        assert!(config.disclaimer_acknowledged);
        assert!(config.onboarding_acknowledged);
        assert_eq!(config.pagination.initial_load, 200);
        assert_eq!(config.pagination.max_limit, 1000);
    }

    #[test]
    fn test_v10_preserves_font_settings() {
        let v10_json = r#"{
            "config_version": "v10",
            "theme": "LIGHT",
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
                "initial_load": 100,
                "max_limit": 500
            },
            "fonts": {
                "ui_font": "ROBOTO",
                "code_font": "CASCADIA_MONO",
                "prose_font": "GEORGIA",
                "disable_ligatures": true
            }
        }"#;

        let config = Config::from(v10_json.to_string());

        assert_eq!(config.config_version, "v10");
        assert_eq!(config.fonts.ui_font, UiFont::Roboto);
        assert_eq!(config.fonts.code_font, CodeFont::CascadiaMono);
        assert_eq!(config.fonts.prose_font, ProseFont::Georgia);
        assert!(config.fonts.disable_ligatures);
    }

    #[test]
    fn test_font_config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.config_version, "v10");
        assert_eq!(parsed.fonts.ui_font, config.fonts.ui_font);
        assert_eq!(parsed.fonts.code_font, config.fonts.code_font);
        assert_eq!(parsed.fonts.prose_font, config.fonts.prose_font);
        assert_eq!(
            parsed.fonts.disable_ligatures,
            config.fonts.disable_ligatures
        );
    }

    #[test]
    fn test_migration_chain_from_older_versions() {
        // Test that old v8 configs still work (will go through v8 -> v9 -> v10)
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

        let config = Config::from(v8_json.to_string());

        assert_eq!(config.config_version, "v10");
        assert_eq!(config.fonts.ui_font, UiFont::Inter);
        assert_eq!(config.fonts.code_font, CodeFont::JetBrainsMono);
    }

    #[test]
    fn test_ui_font_serialization() {
        assert_eq!(serde_json::to_string(&UiFont::Inter).unwrap(), r#""INTER""#);
        assert_eq!(
            serde_json::to_string(&UiFont::Roboto).unwrap(),
            r#""ROBOTO""#
        );
        assert_eq!(
            serde_json::to_string(&UiFont::PublicSans).unwrap(),
            r#""PUBLIC_SANS""#
        );
        assert_eq!(
            serde_json::to_string(&UiFont::ChivoMono).unwrap(),
            r#""CHIVO_MONO""#
        );
        assert_eq!(
            serde_json::to_string(&UiFont::System).unwrap(),
            r#""SYSTEM""#
        );
    }

    #[test]
    fn test_code_font_serialization() {
        assert_eq!(
            serde_json::to_string(&CodeFont::JetBrainsMono).unwrap(),
            r#""JET_BRAINS_MONO""#
        );
        assert_eq!(
            serde_json::to_string(&CodeFont::CascadiaMono).unwrap(),
            r#""CASCADIA_MONO""#
        );
        assert_eq!(serde_json::to_string(&CodeFont::Hack).unwrap(), r#""HACK""#);
        assert_eq!(
            serde_json::to_string(&CodeFont::IbmPlexMono).unwrap(),
            r#""IBM_PLEX_MONO""#
        );
        assert_eq!(
            serde_json::to_string(&CodeFont::ChivoMono).unwrap(),
            r#""CHIVO_MONO""#
        );
        assert_eq!(
            serde_json::to_string(&CodeFont::System).unwrap(),
            r#""SYSTEM""#
        );
    }

    #[test]
    fn test_prose_font_serialization() {
        assert_eq!(
            serde_json::to_string(&ProseFont::Inter).unwrap(),
            r#""INTER""#
        );
        assert_eq!(
            serde_json::to_string(&ProseFont::Roboto).unwrap(),
            r#""ROBOTO""#
        );
        assert_eq!(
            serde_json::to_string(&ProseFont::Georgia).unwrap(),
            r#""GEORGIA""#
        );
        assert_eq!(
            serde_json::to_string(&ProseFont::ChivoMono).unwrap(),
            r#""CHIVO_MONO""#
        );
        assert_eq!(
            serde_json::to_string(&ProseFont::System).unwrap(),
            r#""SYSTEM""#
        );
    }

    #[test]
    fn test_v10_without_fonts_field_uses_defaults() {
        // Configs without fonts field should use defaults via serde(default)
        let v10_json = r#"{
            "config_version": "v10",
            "theme": "SYSTEM",
            "executor_profile": {"executor": "CLAUDE_CODE"},
            "disclaimer_acknowledged": false,
            "onboarding_acknowledged": false,
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
            "workspace_dir": null,
            "last_app_version": null,
            "show_release_notes": false,
            "language": "EN",
            "git_branch_prefix": "vk",
            "dev_banner": {},
            "pagination": {
                "initial_load": 100,
                "max_limit": 500
            }
        }"#;

        let config = Config::from(v10_json.to_string());

        assert_eq!(config.config_version, "v10");
        assert_eq!(config.fonts.ui_font, UiFont::Inter);
        assert_eq!(config.fonts.code_font, CodeFont::JetBrainsMono);
        assert_eq!(config.fonts.prose_font, ProseFont::Inter);
        assert!(!config.fonts.disable_ligatures);
    }
}
