use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

const MCP_BINARY_NAME: &str = "vibe-kanban-mcp";

#[derive(Debug)]
pub struct McpHttpServerProcess {
    child: Child,
}

impl McpHttpServerProcess {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn terminate(&mut self) {
        tracing::info!("[MCP] Terminating HTTP server (PID: {})", self.child.id());
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Err(error) = self.child.kill() {
                    tracing::warn!("[MCP] Failed to kill HTTP server: {}", error);
                }
            }
            Err(error) => {
                tracing::warn!("[MCP] Failed to query HTTP server state: {}", error);
            }
        }

        match self.child.wait() {
            Ok(_) => tracing::info!("[MCP] HTTP server terminated"),
            Err(error) => tracing::warn!("[MCP] Failed to reap HTTP server: {}", error),
        }
    }
}

pub fn spawn_mcp_http_server(
    current_exe: &Path,
    backend_addr: SocketAddr,
) -> Option<McpHttpServerProcess> {
    let mcp_port = match parse_mcp_port(env::var("MCP_PORT").ok().as_deref()) {
        Ok(Some(port)) => port,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!("{}", error);
            return None;
        }
    };

    let binary_path = match resolve_mcp_binary_path(current_exe) {
        Some(path) => path,
        None => {
            tracing::warn!(
                "[MCP] {} binary not found next to {:?}. Build artifacts may be incomplete.",
                MCP_BINARY_NAME,
                current_exe
            );
            return None;
        }
    };

    let backend_url = backend_url_for_mcp(backend_addr);
    let mcp_url = format!("http://{}:{}/mcp", mcp_listener_host(), mcp_port);

    tracing::info!("[MCP] Spawning HTTP server at {}", mcp_url);

    match Command::new(&binary_path)
        .args(["--http", "--port", &mcp_port.to_string()])
        .env("VIBE_BACKEND_URL", &backend_url)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => {
            tracing::info!("[MCP] HTTP server started (PID: {})", child.id());
            Some(McpHttpServerProcess { child })
        }
        Err(error) => {
            tracing::error!("[MCP] Failed to spawn HTTP server: {}", error);
            None
        }
    }
}

fn mcp_listener_host() -> String {
    env::var("MCP_HOST")
        .or_else(|_| env::var("HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn parse_mcp_port(value: Option<&str>) -> Result<Option<u16>, String> {
    let Some(value) = value else {
        return Ok(None);
    };

    let port = value
        .trim()
        .parse::<u16>()
        .map_err(|error| format!("[MCP] Invalid MCP_PORT value '{}': {}", value, error))?;
    if port == 0 {
        return Err("[MCP] Invalid MCP_PORT value '0': expected 1-65535".to_string());
    }
    Ok(Some(port))
}

fn resolve_mcp_binary_path(current_exe: &Path) -> Option<PathBuf> {
    let binary_name = format!("{MCP_BINARY_NAME}{}", env::consts::EXE_SUFFIX);
    current_exe
        .parent()
        .map(|dir| dir.join(binary_name))
        .filter(|path| path.exists())
}

fn backend_url_for_mcp(backend_addr: SocketAddr) -> String {
    let ip = match backend_addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        ip => ip,
    };

    format!("http://{}", SocketAddr::new(ip, backend_addr.port()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    };

    use super::{backend_url_for_mcp, parse_mcp_port, resolve_mcp_binary_path};

    #[test]
    fn parse_mcp_port_returns_none_when_unset() {
        assert_eq!(parse_mcp_port(None).expect("parse should succeed"), None);
    }

    #[test]
    fn parse_mcp_port_accepts_valid_value() {
        assert_eq!(
            parse_mcp_port(Some("8123")).expect("parse should succeed"),
            Some(8123)
        );
    }

    #[test]
    fn parse_mcp_port_rejects_invalid_value() {
        let error = parse_mcp_port(Some("abc")).expect_err("parse should fail");
        assert!(error.contains("Invalid MCP_PORT value"));
    }

    #[test]
    fn parse_mcp_port_rejects_zero() {
        let error = parse_mcp_port(Some("0")).expect_err("parse should fail");
        assert!(error.contains("expected 1-65535"));
    }

    #[test]
    fn backend_url_uses_loopback_for_unspecified_ipv4() {
        let url = backend_url_for_mcp(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 4321)));
        assert_eq!(url, "http://127.0.0.1:4321");
    }

    #[test]
    fn backend_url_uses_loopback_for_unspecified_ipv6() {
        let url = backend_url_for_mcp(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 4321)));
        assert_eq!(url, "http://127.0.0.1:4321");
    }

    #[test]
    fn resolve_mcp_binary_path_uses_current_exe_directory() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let exe_path = temp_dir
            .path()
            .join(format!("server{}", std::env::consts::EXE_SUFFIX));
        let mcp_path = temp_dir
            .path()
            .join(format!("vibe-kanban-mcp{}", std::env::consts::EXE_SUFFIX));
        fs::write(&exe_path, b"").expect("write exe");
        fs::write(&mcp_path, b"").expect("write mcp binary");

        let resolved = resolve_mcp_binary_path(&exe_path).expect("binary path should resolve");
        assert_eq!(resolved, mcp_path);
    }
}
