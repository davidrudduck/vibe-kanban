use anyhow::Error;
use executors::{executors::BaseCodingAgent, profile::ExecutorProfileId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use v8::{
    EditorConfig, EditorType, GitHubConfig, NotificationConfig, SendMessageShortcut, ShowcaseState,
    SoundFile, ThemeMode, UiLanguage,
};

use crate::services::config::versions::v8;

fn default_git_branch_prefix() -> String {
    "vk".to_string()
}

fn default_pr_auto_description_enabled() -> bool {
    true
}

fn default_commit_reminder_enabled() -> bool {
    true
}

fn default_relay_enabled() -> bool {
    true
}

fn default_discord_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum UiFont {
    #[default]
    IbmPlexSans,
    Inter,
    Roboto,
    PublicSans,
    System,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum CodeFont {
    #[default]
    IbmPlexMono,
    JetBrainsMono,
    CascadiaMono,
    Hack,
    System,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum ProseFont {
    #[default]
    IbmPlexSans,
    Roboto,
    Georgia,
    System,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct FontConfig {
    #[serde(default)]
    pub ui_font: UiFont,
    #[serde(default)]
    pub code_font: CodeFont,
    #[serde(default)]
    pub prose_font: ProseFont,
    #[serde(default)]
    pub disable_ligatures: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct HostBannerConfig {
    #[serde(default)]
    pub show_hostname: bool,
    #[serde(default)]
    pub show_os_info: bool,
    #[serde(default)]
    pub env_label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct LinksConfig {
    #[serde(default = "default_discord_enabled")]
    pub discord_enabled: bool,
    #[serde(default)]
    pub discord_url: Option<String>,
    #[serde(default)]
    pub feedback_url: Option<String>,
}

impl Default for LinksConfig {
    fn default() -> Self {
        Self {
            discord_enabled: true,
            discord_url: None,
            feedback_url: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct AppearanceConfig {
    #[serde(default)]
    pub fonts: FontConfig,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub host_banner: HostBannerConfig,
    #[serde(default)]
    pub links: LinksConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Config {
    pub config_version: String,
    pub theme: ThemeMode,
    pub executor_profile: ExecutorProfileId,
    pub disclaimer_acknowledged: bool,
    pub onboarding_acknowledged: bool,
    #[serde(default)]
    pub remote_onboarding_acknowledged: bool,
    pub notifications: NotificationConfig,
    pub editor: EditorConfig,
    pub github: GitHubConfig,
    pub analytics_enabled: bool,
    pub workspace_dir: Option<String>,
    pub last_app_version: Option<String>,
    pub show_release_notes: bool,
    #[serde(default)]
    pub language: UiLanguage,
    #[serde(default = "default_git_branch_prefix")]
    pub git_branch_prefix: String,
    #[serde(default)]
    pub showcases: ShowcaseState,
    #[serde(default = "default_pr_auto_description_enabled")]
    pub pr_auto_description_enabled: bool,
    #[serde(default)]
    pub pr_auto_description_prompt: Option<String>,
    #[serde(default = "default_commit_reminder_enabled")]
    pub commit_reminder_enabled: bool,
    #[serde(default)]
    pub commit_reminder_prompt: Option<String>,
    #[serde(default)]
    pub send_message_shortcut: SendMessageShortcut,
    #[serde(default = "default_relay_enabled")]
    pub relay_enabled: bool,
    #[serde(default)]
    pub host_nickname: Option<String>,
    #[serde(default)]
    pub appearance: AppearanceConfig,
}

impl Config {
    fn from_v8_config(old_config: v8::Config) -> Self {
        Self {
            config_version: "v9".to_string(),
            theme: old_config.theme,
            executor_profile: old_config.executor_profile,
            disclaimer_acknowledged: old_config.disclaimer_acknowledged,
            onboarding_acknowledged: old_config.onboarding_acknowledged,
            remote_onboarding_acknowledged: old_config.remote_onboarding_acknowledged,
            notifications: old_config.notifications,
            editor: old_config.editor,
            github: old_config.github,
            analytics_enabled: old_config.analytics_enabled,
            workspace_dir: old_config.workspace_dir,
            last_app_version: old_config.last_app_version,
            show_release_notes: old_config.show_release_notes,
            language: old_config.language,
            git_branch_prefix: old_config.git_branch_prefix,
            showcases: old_config.showcases,
            pr_auto_description_enabled: old_config.pr_auto_description_enabled,
            pr_auto_description_prompt: old_config.pr_auto_description_prompt,
            commit_reminder_enabled: old_config.commit_reminder_enabled,
            commit_reminder_prompt: old_config.commit_reminder_prompt,
            send_message_shortcut: old_config.send_message_shortcut,
            relay_enabled: old_config.relay_enabled,
            host_nickname: old_config.host_nickname,
            appearance: AppearanceConfig::default(),
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
            remote_onboarding_acknowledged: false,
            notifications: NotificationConfig::default(),
            editor: EditorConfig::default(),
            github: GitHubConfig::default(),
            analytics_enabled: true,
            workspace_dir: None,
            last_app_version: None,
            show_release_notes: false,
            language: UiLanguage::default(),
            git_branch_prefix: default_git_branch_prefix(),
            showcases: ShowcaseState::default(),
            pr_auto_description_enabled: true,
            pr_auto_description_prompt: None,
            commit_reminder_enabled: true,
            commit_reminder_prompt: None,
            send_message_shortcut: SendMessageShortcut::default(),
            relay_enabled: true,
            host_nickname: None,
            appearance: AppearanceConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v8_migrates_to_v9_with_default_appearance() {
        let v8_config = v8::Config::default();
        let v8_json = serde_json::to_string(&v8_config).unwrap();

        let v9_config = Config::from(v8_json);

        assert_eq!(v9_config.config_version, "v9");
        assert_eq!(v9_config.appearance, AppearanceConfig::default());
        assert_eq!(v9_config.appearance.fonts.ui_font, UiFont::IbmPlexSans);
        assert_eq!(v9_config.appearance.fonts.code_font, CodeFont::IbmPlexMono);
        assert_eq!(
            v9_config.appearance.fonts.prose_font,
            ProseFont::IbmPlexSans
        );
        assert!(!v9_config.appearance.fonts.disable_ligatures);
        assert!(!v9_config.appearance.host_banner.show_hostname);
        assert!(!v9_config.appearance.host_banner.show_os_info);
        assert!(v9_config.appearance.links.discord_enabled);
    }

    #[test]
    fn test_v9_preserves_appearance_settings() {
        let mut config = Config::default();
        config.appearance.fonts.ui_font = UiFont::Inter;
        config.appearance.fonts.code_font = CodeFont::JetBrainsMono;
        config.appearance.fonts.disable_ligatures = true;
        config.appearance.accent_color = Some("25 82% 54%".to_string());
        config.appearance.host_banner.show_hostname = true;
        config.appearance.links.discord_enabled = false;

        let json = serde_json::to_string(&config).unwrap();
        let restored = Config::from(json);

        assert_eq!(restored.config_version, "v9");
        assert_eq!(restored.appearance.fonts.ui_font, UiFont::Inter);
        assert_eq!(restored.appearance.fonts.code_font, CodeFont::JetBrainsMono);
        assert!(restored.appearance.fonts.disable_ligatures);
        assert_eq!(
            restored.appearance.accent_color,
            Some("25 82% 54%".to_string())
        );
        assert!(restored.appearance.host_banner.show_hostname);
        assert!(!restored.appearance.links.discord_enabled);
    }
}
