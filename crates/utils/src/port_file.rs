//! Instance registry for multi-instance vibe-kanban support.
//!
//! When multiple vibe-kanban instances run simultaneously (different projects),
//! each registers itself with its project root, PID, and all service ports.
//! This enables:
//! - Safe process management (stop specific instance by project)
//! - Executor discovery (find which instance spawned a worktree)
//! - Port conflict detection

use std::{
    env,
    path::{Path, PathBuf},
    process,
};

use sha2::{Digest, Sha256};
use tokio::fs;

/// Complete instance information including all service ports.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InstanceInfo {
    /// Canonical path to the project root directory
    pub project_root: PathBuf,

    /// Process ID of the server
    pub pid: u32,

    /// Binary name (e.g., "vks-node-server")
    pub binary: String,

    /// When this instance started (RFC 3339)
    pub started_at: String,

    /// All service ports for this instance
    pub ports: InstancePorts,

    /// Optional human-readable instance name
    pub name: Option<String>,
}

/// All ports used by a vibe-kanban instance.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct InstancePorts {
    /// Backend API server port
    pub backend: Option<u16>,

    /// Frontend dev server port (only in dev mode)
    pub frontend: Option<u16>,

    /// MCP HTTP server port (if enabled)
    pub mcp: Option<u16>,

    /// Remote/Hive WebSocket port (if enabled)
    pub hive: Option<u16>,
}

impl InstanceInfo {
    /// Create a new instance info for the current process.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            pid: process::id(),
            binary: env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or_else(|| "vks-node-server".to_string()),
            started_at: chrono::Utc::now().to_rfc3339(),
            ports: InstancePorts::default(),
            name: None,
        }
    }

    /// Check if this instance's process is still running.
    pub fn is_running(&self) -> bool {
        #[cfg(unix)]
        {
            Path::new(&format!("/proc/{}", self.pid)).exists()
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, assume running (conservative)
            true
        }
    }

    /// Get the backend URL for this instance.
    pub fn backend_url(&self) -> Option<String> {
        self.ports
            .backend
            .map(|p| format!("http://127.0.0.1:{}", p))
    }

    /// Get the MCP URL for this instance.
    pub fn mcp_url(&self) -> Option<String> {
        self.ports.mcp.map(|p| format!("http://127.0.0.1:{}/mcp", p))
    }
}

/// Registry for all running vibe-kanban instances.
pub struct InstanceRegistry;

impl InstanceRegistry {
    /// Directory where instance files are stored.
    fn registry_dir() -> PathBuf {
        env::temp_dir().join("vibe-kanban").join("instances")
    }

    /// Generate a unique filename for a project path.
    fn instance_filename(project_root: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(project_root.to_string_lossy().as_bytes());
        let hash = hasher.finalize();
        format!("{:x}.json", &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64))
    }

    /// Path to the instance file for a project.
    fn instance_path(project_root: &Path) -> PathBuf {
        Self::registry_dir().join(Self::instance_filename(project_root))
    }

    /// Register an instance in the registry.
    pub async fn register(info: &InstanceInfo) -> std::io::Result<PathBuf> {
        let dir = Self::registry_dir();
        fs::create_dir_all(&dir).await?;

        let path = Self::instance_path(&info.project_root);
        let json = serde_json::to_string_pretty(info)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        fs::write(&path, &json).await?;

        tracing::info!(
            project = %info.project_root.display(),
            pid = info.pid,
            backend_port = ?info.ports.backend,
            "Registered vibe-kanban instance"
        );

        // Also write legacy port file for backwards compatibility
        let legacy_port_path = env::temp_dir().join("vibe-kanban").join("vibe-kanban.port");
        if let Some(port) = info.ports.backend {
            fs::write(&legacy_port_path, port.to_string()).await?;
        }

        Ok(path)
    }

    /// Update ports for an existing instance.
    pub async fn update_ports(project_root: &Path, ports: InstancePorts) -> std::io::Result<()> {
        let path = Self::instance_path(project_root);

        if let Ok(content) = fs::read_to_string(&path).await
            && let Ok(mut info) = serde_json::from_str::<InstanceInfo>(&content)
        {
            info.ports = ports;
            let json = serde_json::to_string_pretty(&info)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            fs::write(&path, json).await?;
        }

        Ok(())
    }

    /// Unregister an instance (on shutdown).
    pub async fn unregister(project_root: &Path) -> std::io::Result<()> {
        let path = Self::instance_path(project_root);
        if path.exists() {
            fs::remove_file(&path).await?;
            tracing::debug!(project = %project_root.display(), "Unregistered instance");
        }
        Ok(())
    }

    /// Get instance info for a specific project.
    pub async fn get(project_root: &Path) -> std::io::Result<InstanceInfo> {
        let path = Self::instance_path(project_root);
        let content = fs::read_to_string(&path).await?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// List all registered instances (including stale ones).
    pub async fn list_all() -> std::io::Result<Vec<InstanceInfo>> {
        let dir = Self::registry_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut instances = Vec::new();
        let mut entries = fs::read_dir(&dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Ok(content) = fs::read_to_string(&path).await
                && let Ok(info) = serde_json::from_str::<InstanceInfo>(&content)
            {
                instances.push(info);
            }
        }

        Ok(instances)
    }

    /// List only running instances (filters out stale entries).
    pub async fn list_running() -> std::io::Result<Vec<InstanceInfo>> {
        let all = Self::list_all().await?;
        Ok(all.into_iter().filter(|i| i.is_running()).collect())
    }

    /// Clean up stale instance entries (processes that are no longer running).
    pub async fn cleanup_stale() -> std::io::Result<usize> {
        let dir = Self::registry_dir();
        if !dir.exists() {
            return Ok(0);
        }

        let mut removed = 0;
        let mut entries = fs::read_dir(&dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Ok(content) = fs::read_to_string(&path).await
                && let Ok(info) = serde_json::from_str::<InstanceInfo>(&content)
                && !info.is_running()
                && fs::remove_file(&path).await.is_ok()
            {
                tracing::debug!(
                    project = %info.project_root.display(),
                    pid = info.pid,
                    "Removed stale instance entry"
                );
                removed += 1;
            }
        }

        if removed > 0 {
            tracing::info!(count = removed, "Cleaned up stale instance entries");
        }

        Ok(removed)
    }

    /// Find the instance that owns a given working directory.
    ///
    /// This is useful for executors running in worktrees to find their parent instance.
    /// It walks up the directory tree looking for a registered project root.
    pub async fn find_by_working_dir(working_dir: &Path) -> std::io::Result<Option<InstanceInfo>> {
        let instances = Self::list_running().await?;
        let canonical = working_dir.canonicalize().unwrap_or_else(|_| working_dir.to_path_buf());

        // First, check if any instance's project root is a prefix of the working dir
        for info in &instances {
            if canonical.starts_with(&info.project_root) {
                return Ok(Some(info.clone()));
            }
        }

        // Check worktree patterns: /var/tmp/vibe-kanban/worktrees/<project-id>/...
        // The project-id in the path should match a registered project
        if let Some(worktree_base) = canonical
            .to_string_lossy()
            .find("/vibe-kanban/worktrees/")
        {
            let after_base = &canonical.to_string_lossy()[worktree_base + 23..]; // Skip "/vibe-kanban/worktrees/"
            if let Some(slash_pos) = after_base.find('/') {
                let _project_id = &after_base[..slash_pos];
                // TODO: Match project_id to registered instances via project UUID
                // For now, return the most recently started instance as a fallback
                if let Some(newest) = instances.into_iter().max_by_key(|i| i.started_at.clone()) {
                    return Ok(Some(newest));
                }
            }
        }

        Ok(None)
    }

    /// Stop an instance by its project root.
    #[cfg(unix)]
    pub async fn stop(project_root: &Path) -> std::io::Result<bool> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let info = Self::get(project_root).await?;
        let pid = Pid::from_raw(info.pid as i32);

        match kill(pid, Signal::SIGTERM) {
            Ok(()) => {
                tracing::info!(
                    project = %project_root.display(),
                    pid = info.pid,
                    "Sent SIGTERM to vibe-kanban instance"
                );
                Ok(true)
            }
            Err(nix::errno::Errno::ESRCH) => {
                tracing::warn!(pid = info.pid, "Instance process not found (already stopped?)");
                // Clean up the stale entry
                let _ = Self::unregister(project_root).await;
                Ok(false)
            }
            Err(e) => Err(std::io::Error::other(format!(
                "Failed to send signal: {}",
                e
            ))),
        }
    }

    #[cfg(not(unix))]
    pub async fn stop(_project_root: &Path) -> std::io::Result<bool> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Signal handling not supported on this platform",
        ))
    }
}

// ============================================================================
// Legacy compatibility functions
// ============================================================================

/// Legacy: Server info stored in the info file (JSON format)
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ServerInfo {
    pub port: u16,
    pub pid: u32,
    pub started_at: String,
    pub binary: String,
}

/// Legacy: Write port file (for backwards compatibility).
pub async fn write_port_file(port: u16) -> std::io::Result<PathBuf> {
    let dir = env::temp_dir().join("vibe-kanban");
    let path = dir.join("vibe-kanban.port");
    tracing::debug!("Writing port {} to {:?}", port, path);
    fs::create_dir_all(&dir).await?;
    fs::write(&path, port.to_string()).await?;

    // Also write legacy info file
    let info_path = dir.join("vibe-kanban.info");
    let info = ServerInfo {
        port,
        pid: process::id(),
        started_at: chrono::Utc::now().to_rfc3339(),
        binary: env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "vks-node-server".to_string()),
    };
    let info_json = serde_json::to_string_pretty(&info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&info_path, info_json).await?;

    Ok(path)
}

/// Legacy: Read server info from the info file
pub async fn read_server_info() -> std::io::Result<ServerInfo> {
    let dir = env::temp_dir().join("vibe-kanban");
    let path = dir.join("vibe-kanban.info");
    let content = fs::read_to_string(&path).await?;
    serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Legacy: Check if the server process is still running
pub async fn is_server_running() -> bool {
    match read_server_info().await {
        Ok(info) => Path::new(&format!("/proc/{}", info.pid)).exists(),
        Err(_) => false,
    }
}

/// Legacy: Stop the running server gracefully using its PID
pub async fn stop_server() -> std::io::Result<bool> {
    let info = read_server_info().await?;

    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(info.pid as i32);
        match kill(pid, Signal::SIGTERM) {
            Ok(()) => {
                tracing::info!("Sent SIGTERM to vibe-kanban server (PID: {})", info.pid);
                Ok(true)
            }
            Err(nix::errno::Errno::ESRCH) => {
                tracing::warn!("Server process {} not found (already stopped?)", info.pid);
                Ok(false)
            }
            Err(e) => Err(std::io::Error::other(format!(
                "Failed to send signal: {}",
                e
            ))),
        }
    }

    #[cfg(not(unix))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Signal handling not supported on this platform",
        ))
    }
}

pub async fn read_port_file(app_name: &str) -> std::io::Result<u16> {
    let dir = env::temp_dir().join(app_name);
    let path = dir.join(format!("{app_name}.port"));
    tracing::debug!("Reading port from {:?}", path);

    let content = fs::read_to_string(&path).await?;
    let port: u16 = content
        .trim()
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(port)
}
