use std::{env, path::PathBuf, process};

use tokio::fs;

/// Server info stored in the info file (JSON format)
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ServerInfo {
    pub port: u16,
    pub pid: u32,
    pub started_at: String,
    pub binary: String,
}

pub async fn write_port_file(port: u16) -> std::io::Result<PathBuf> {
    let dir = env::temp_dir().join("vibe-kanban");
    let path = dir.join("vibe-kanban.port");
    tracing::debug!("Writing port {} to {:?}", port, path);
    fs::create_dir_all(&dir).await?;
    fs::write(&path, port.to_string()).await?;

    // Also write a JSON info file with PID for safe process management
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
    tracing::debug!("Wrote server info to {:?}", info_path);

    Ok(path)
}

/// Read server info from the info file
pub async fn read_server_info() -> std::io::Result<ServerInfo> {
    let dir = env::temp_dir().join("vibe-kanban");
    let path = dir.join("vibe-kanban.info");
    let content = fs::read_to_string(&path).await?;
    serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Check if the server process is still running
pub async fn is_server_running() -> bool {
    match read_server_info().await {
        Ok(info) => {
            // Check if process with this PID exists
            std::path::Path::new(&format!("/proc/{}", info.pid)).exists()
        }
        Err(_) => false,
    }
}

/// Stop the running server gracefully using its PID
pub async fn stop_server() -> std::io::Result<bool> {
    let info = read_server_info().await?;

    // Send SIGTERM to the specific PID
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
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
