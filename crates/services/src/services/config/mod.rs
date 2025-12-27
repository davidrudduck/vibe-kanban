use std::path::PathBuf;

use thiserror::Error;

pub mod editor;
mod versions;

pub use editor::EditorOpenError;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

pub type Config = versions::v11::Config;
pub type NotificationConfig = versions::v11::NotificationConfig;
pub type EditorConfig = versions::v11::EditorConfig;
pub type ThemeMode = versions::v11::ThemeMode;
pub type SoundFile = versions::v11::SoundFile;
pub type EditorType = versions::v11::EditorType;
pub type GitHubConfig = versions::v11::GitHubConfig;
pub type UiLanguage = versions::v11::UiLanguage;
pub type DevBannerConfig = versions::v11::DevBannerConfig;
pub type PaginationConfig = versions::v11::PaginationConfig;
pub type FontConfig = versions::v11::FontConfig;
pub type UiFont = versions::v11::UiFont;
pub type CodeFont = versions::v11::CodeFont;
pub type ProseFont = versions::v11::ProseFont;
pub type TimestampConfig = versions::v11::TimestampConfig;

/// Will always return config, trying old schemas or eventually returning default
pub async fn load_config_from_file(config_path: &PathBuf) -> Config {
    match std::fs::read_to_string(config_path) {
        Ok(raw_config) => Config::from(raw_config),
        Err(_) => {
            tracing::info!("No config file found, creating one");
            Config::default()
        }
    }
}

/// Saves the config to the given path
pub async fn save_config_to_file(
    config: &Config,
    config_path: &PathBuf,
) -> Result<(), ConfigError> {
    let raw_config = serde_json::to_string_pretty(config)?;
    std::fs::write(config_path, raw_config)?;
    Ok(())
}
