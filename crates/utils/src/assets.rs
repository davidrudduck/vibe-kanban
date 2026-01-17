use directories::ProjectDirs;
use rust_embed::RustEmbed;

const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");

pub fn asset_dir() -> std::path::PathBuf {
    let path = if cfg!(debug_assertions) {
        std::path::PathBuf::from(PROJECT_ROOT).join("../../dev_assets")
    } else {
        ProjectDirs::from("ai", "bloop", "vibe-kanban")
            .expect("OS didn't give us a home directory")
            .data_dir()
            .to_path_buf()
    };

    // Ensure the directory exists
    if !path.exists() {
        std::fs::create_dir_all(&path).expect("Failed to create asset directory");
    }

    path
    // ✔ macOS → ~/Library/Application Support/MyApp
    // ✔ Linux → ~/.local/share/myapp   (respects XDG_DATA_HOME)
    // ✔ Windows → %APPDATA%\Example\MyApp
}

/// Get the configuration directory path.
///
/// Respects the `VK_CONFIG_DIR` environment variable for custom locations.
/// Supports tilde expansion (e.g., `~/vibe-kanban/config`).
///
/// Default: `{asset_dir}`
pub fn config_dir() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("VK_CONFIG_DIR") {
        let expanded = crate::path::expand_tilde(&path);
        // Ensure the directory exists
        if !expanded.exists() {
            std::fs::create_dir_all(&expanded).expect("Failed to create config directory");
        }
        return expanded;
    }
    asset_dir()
}

pub fn config_path() -> std::path::PathBuf {
    config_dir().join("config.json")
}

pub fn profiles_path() -> std::path::PathBuf {
    config_dir().join("profiles.json")
}

pub fn credentials_path() -> std::path::PathBuf {
    config_dir().join("credentials.json")
}

/// Get the database file path.
///
/// Respects the `VK_DATABASE_PATH` environment variable for custom locations.
/// Supports tilde expansion (e.g., `~/vibe-kanban/db.sqlite`).
///
/// Default: `{asset_dir}/db.sqlite`
pub fn database_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("VK_DATABASE_PATH") {
        return crate::path::expand_tilde(&path);
    }
    asset_dir().join("db.sqlite")
}

/// Get the backup directory path.
///
/// Respects the `VK_BACKUP_DIR` environment variable for custom locations.
/// Supports tilde expansion (e.g., `~/vibe-kanban/backups`).
///
/// Default: `{asset_dir}/backups`
pub fn backup_dir() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("VK_BACKUP_DIR") {
        return crate::path::expand_tilde(&path);
    }
    asset_dir().join("backups")
}

#[derive(RustEmbed)]
#[folder = "../../assets/sounds"]
pub struct SoundAssets;

#[derive(RustEmbed)]
#[folder = "../../assets/scripts"]
pub struct ScriptAssets;

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    #[test]
    #[serial]
    fn test_database_path_default() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::remove_var("VK_DATABASE_PATH") };
        let path = database_path();
        assert!(path.ends_with("db.sqlite"));
    }

    #[test]
    #[serial]
    fn test_database_path_env_override() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_DATABASE_PATH", "/custom/path/test.db") };
        let path = database_path();
        unsafe { env::remove_var("VK_DATABASE_PATH") };
        assert_eq!(path, std::path::PathBuf::from("/custom/path/test.db"));
    }

    #[test]
    #[serial]
    fn test_database_path_tilde_expansion() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_DATABASE_PATH", "~/vibe-kanban/db.sqlite") };
        let path = database_path();
        unsafe { env::remove_var("VK_DATABASE_PATH") };
        assert!(!path.to_string_lossy().contains('~'));
        assert!(path.is_absolute());
    }

    #[test]
    #[serial]
    fn test_backup_dir_default() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::remove_var("VK_BACKUP_DIR") };
        let dir = backup_dir();
        assert!(dir.ends_with("backups"));
    }

    #[test]
    #[serial]
    fn test_backup_dir_env_override() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_BACKUP_DIR", "/custom/backups") };
        let dir = backup_dir();
        unsafe { env::remove_var("VK_BACKUP_DIR") };
        assert_eq!(dir, std::path::PathBuf::from("/custom/backups"));
    }

    #[test]
    #[serial]
    fn test_backup_dir_tilde_expansion() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_BACKUP_DIR", "~/my-backups") };
        let dir = backup_dir();
        unsafe { env::remove_var("VK_BACKUP_DIR") };
        assert!(!dir.to_string_lossy().contains('~'));
        assert!(dir.is_absolute());
    }

    #[test]
    #[serial]
    fn test_config_dir_default() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::remove_var("VK_CONFIG_DIR") };
        let dir = config_dir();
        // Default should be asset_dir
        assert_eq!(dir, asset_dir());
    }

    #[test]
    #[serial]
    fn test_config_dir_env_override() {
        let temp = tempfile::tempdir().unwrap();
        let custom_path = temp.path().join("custom-config");
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_CONFIG_DIR", custom_path.to_str().unwrap()) };
        let dir = config_dir();
        unsafe { env::remove_var("VK_CONFIG_DIR") };
        assert_eq!(dir, custom_path);
        // Directory should be created automatically
        assert!(custom_path.exists());
    }

    #[test]
    #[serial]
    fn test_config_dir_tilde_expansion() {
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_CONFIG_DIR", "~/my-config") };
        let dir = config_dir();
        unsafe { env::remove_var("VK_CONFIG_DIR") };
        assert!(!dir.to_string_lossy().contains('~'));
        assert!(dir.is_absolute());
    }

    #[test]
    #[serial]
    fn test_config_paths_use_config_dir() {
        let temp = tempfile::tempdir().unwrap();
        let custom_path = temp.path().join("custom-config");
        // SAFETY: Tests run serially via #[serial] attribute
        unsafe { env::set_var("VK_CONFIG_DIR", custom_path.to_str().unwrap()) };

        let config = config_path();
        let profiles = profiles_path();
        let credentials = credentials_path();

        unsafe { env::remove_var("VK_CONFIG_DIR") };

        assert_eq!(config, custom_path.join("config.json"));
        assert_eq!(profiles, custom_path.join("profiles.json"));
        assert_eq!(credentials, custom_path.join("credentials.json"));
    }
}
