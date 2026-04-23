use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use mcp::task_server::McpServer;
use rmcp::{
    ServiceExt,
    transport::{
        stdio,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use tracing_subscriber::{EnvFilter, prelude::*};
use utils::{
    port_file::{PortInfo, read_port_info},
    sentry::{self as sentry_utils, SentrySource, sentry_layer},
};

const HOST_ENV: &str = "MCP_HOST";
const PORT_ENV: &str = "MCP_PORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpLaunchMode {
    Global,
    Orchestrator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum McpTransport {
    Stdio,
    Http { port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchConfig {
    mode: McpLaunchMode,
    transport: McpTransport,
    host: Option<String>,
    backend_url: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let launch_config = resolve_launch_config()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let version = env!("CARGO_PKG_VERSION");
            init_process_logging("vibe-kanban-mcp", version);

            let base_url = resolve_base_url("vibe-kanban-mcp", &launch_config).await?;
            let LaunchConfig {
                mode, transport, ..
            } = launch_config;

            let server = match mode {
                McpLaunchMode::Global => McpServer::new_global(&base_url),
                McpLaunchMode::Orchestrator => McpServer::new_orchestrator(&base_url),
            };

            match transport {
                McpTransport::Stdio => {
                    let service = server.init().await?.serve(stdio()).await.map_err(|error| {
                        tracing::error!("serving error: {:?}", error);
                        error
                    })?;

                    service.waiting().await?;
                    Ok(())
                }
                McpTransport::Http { port } => run_http_server(server, port).await,
            }
        })
}

fn resolve_launch_config() -> anyhow::Result<LaunchConfig> {
    resolve_launch_config_from_iter(std::env::args().skip(1))
}

fn resolve_launch_config_from_iter<I>(mut args: I) -> anyhow::Result<LaunchConfig>
where
    I: Iterator<Item = String>,
{
    let mut mode = None;
    let mut http = false;
    let mut port = None;
    let mut host = None;
    let mut backend_url = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                mode = Some(args.next().ok_or_else(|| {
                    anyhow::anyhow!("Missing value for --mode. Expected 'global' or 'orchestrator'")
                })?);
            }
            "--http" => {
                http = true;
            }
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing value for --port"))?;
                let parsed_port = value
                    .parse::<u16>()
                    .map_err(|_| anyhow::anyhow!("Invalid value for --port: '{value}'"))?;
                if parsed_port == 0 {
                    anyhow::bail!("Invalid value for --port: '{value}'. Expected 1-65535");
                }
                port = Some(parsed_port);
            }
            "--host" => {
                host = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing value for --host"))?,
                );
            }
            "--backend-url" => {
                backend_url = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing value for --backend-url"))?,
                );
            }
            "-h" | "--help" => {
                println!(
                    "Usage: vibe-kanban-mcp [--mode <global|orchestrator>] [--http --port <port>] [--host <HOST>] [--backend-url <URL>]"
                );
                std::process::exit(0);
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown argument '{arg}'. Usage: vibe-kanban-mcp [--mode <global|orchestrator>] [--http --port <port>] [--host <HOST>] [--backend-url <URL>]"
                ));
            }
        }
    }

    let mode = match mode
        .as_deref()
        .unwrap_or("global")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "global" => McpLaunchMode::Global,
        "orchestrator" => McpLaunchMode::Orchestrator,
        value => {
            return Err(anyhow::anyhow!(
                "Invalid MCP mode '{value}'. Expected 'global' or 'orchestrator'"
            ));
        }
    };

    let transport = match (http, port) {
        (false, None) => McpTransport::Stdio,
        (false, Some(_)) => {
            anyhow::bail!("--port requires --http");
        }
        (true, None) => {
            anyhow::bail!("Missing value for --port");
        }
        (true, Some(port)) => McpTransport::Http { port },
    };

    Ok(LaunchConfig {
        mode,
        transport,
        host,
        backend_url,
    })
}

async fn remap_session_not_found(req: Request<Body>, next: Next) -> Response {
    let response = next.run(req).await;
    if response.status() == StatusCode::UNAUTHORIZED {
        let (mut parts, body) = response.into_parts();
        parts.status = StatusCode::NOT_FOUND;
        Response::from_parts(parts, body)
    } else {
        response
    }
}

async fn run_http_server(server: McpServer, port: u16) -> anyhow::Result<()> {
    let bind_host = std::env::var(HOST_ENV)
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let bind_address = format!("{bind_host}:{port}");

    tracing::info!("[vibe-kanban-mcp] Starting HTTP server at http://{bind_address}/mcp");

    let template_server = Arc::new(server.init().await?);
    let session_manager = Arc::new(LocalSessionManager::default());
    let service: StreamableHttpService<McpServer, LocalSessionManager> = StreamableHttpService::new(
        move || Ok((*template_server).clone()),
        session_manager,
        StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn(remap_session_not_found));
    let tcp_listener = tokio::net::TcpListener::bind(&bind_address).await?;

    tracing::info!("[vibe-kanban-mcp] HTTP server listening at http://{bind_address}/mcp");

    axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.unwrap();
            tracing::info!("[vibe-kanban-mcp] Received shutdown signal, stopping HTTP server...");
        })
        .await?;

    Ok(())
}

async fn resolve_base_url(log_prefix: &str, config: &LaunchConfig) -> anyhow::Result<String> {
    // Single, unambiguous precedence chain for backend URL resolution:
    //   1. `config.backend_url` (explicit --backend-url CLI arg)
    //   2. `VIBE_BACKEND_URL` env var (must parse as a URL *with a scheme*)
    //   3. Port file `backend_url` field
    //   4. Reconstructed from HOST / (MCP_PORT|BACKEND_PORT|PORT)
    //
    // `HOST` and port env vars apply uniformly across all paths where we
    // reconstruct a URL; they never override a fully-formed explicit URL.
    if let Some(url) = &config.backend_url {
        tracing::info!(
            "[{}] Using backend URL from --backend-url: {}",
            log_prefix,
            url
        );
        return Ok(url.clone());
    }

    let host_override = config
        .host
        .clone()
        .or_else(|| std::env::var(HOST_ENV).ok())
        .or_else(|| std::env::var("HOST").ok());

    // VIBE_BACKEND_URL: only honor if it parses as a URL with a scheme.
    // Invalid (e.g. missing-scheme) values fall through to the next source
    // with a warning rather than being passed through as raw strings.
    let backend_url_env =
        std::env::var("VIBE_BACKEND_URL")
            .ok()
            .and_then(|raw| match url::Url::parse(&raw) {
                Ok(parsed) if !parsed.scheme().is_empty() => Some(parsed),
                Ok(_) => {
                    tracing::warn!(
                        "[{}] Ignoring VIBE_BACKEND_URL='{}': missing scheme",
                        log_prefix,
                        raw
                    );
                    None
                }
                Err(e) => {
                    tracing::warn!("[{}] Ignoring VIBE_BACKEND_URL='{}': {e}", log_prefix, raw);
                    None
                }
            });

    if let Some(mut base) = backend_url_env {
        // Allow HOST / port env vars to override components of the parsed URL
        // so both branches have the same precedence rules. `set_host` /
        // `set_port` reject values the URL scheme can't represent (e.g.
        // can't set a host on a cannot-be-a-base URL); surface that as a
        // warning instead of silently ignoring the user's override.
        if let Some(h) = &host_override
            && base.set_host(Some(h)).is_err()
        {
            tracing::warn!(
                "[{}] Could not apply HOST override '{}' to VIBE_BACKEND_URL; using original host",
                log_prefix,
                h
            );
        }
        if let Some(p) = read_port_override_env()?
            && base.set_port(Some(p)).is_err()
        {
            tracing::warn!(
                "[{}] Could not apply port override {} to VIBE_BACKEND_URL; using original port",
                log_prefix,
                p
            );
        }
        let url = base.as_str().trim_end_matches('/').to_string();
        tracing::info!(
            "[{}] Using backend URL from VIBE_BACKEND_URL: {}",
            log_prefix,
            url
        );
        return Ok(url);
    }

    let host = host_override.unwrap_or_else(|| "127.0.0.1".to_string());
    let explicit_port = read_port_override_env()?;

    let port_info = match explicit_port {
        Some(_) => None,
        None => Some(read_port_info("vibe-kanban").await?),
    };

    resolve_base_url_from_sources(log_prefix, None, host, explicit_port, port_info)
}

/// Read port override from env (MCP_PORT, then BACKEND_PORT, then PORT).
/// Returns `Ok(None)` when nothing is set; errors on unparseable values.
fn read_port_override_env() -> anyhow::Result<Option<u16>> {
    match std::env::var(PORT_ENV)
        .or_else(|_| std::env::var("BACKEND_PORT"))
        .or_else(|_| std::env::var("PORT"))
    {
        Ok(port_str) => Ok(Some(port_str.parse::<u16>().map_err(|error| {
            anyhow::anyhow!("Invalid port value '{}': {}", port_str, error)
        })?)),
        Err(_) => Ok(None),
    }
}

fn resolve_base_url_from_sources(
    log_prefix: &str,
    backend_url: Option<String>,
    host: String,
    explicit_port: Option<u16>,
    port_info: Option<PortInfo>,
) -> anyhow::Result<String> {
    if let Some(url) = backend_url {
        tracing::info!(
            "[{}] Using backend URL from VIBE_BACKEND_URL: {}",
            log_prefix,
            url
        );
        return Ok(url);
    }

    if let Some(port) = explicit_port {
        tracing::info!("[{}] Using port from environment: {}", log_prefix, port);
        let url = format!("http://{}:{}", host, port);
        tracing::info!("[{}] Using backend URL: {}", log_prefix, url);
        return Ok(url);
    }

    let port_info = port_info.ok_or_else(|| anyhow::anyhow!("Missing port file information"))?;
    if let Some(url) = port_info.backend_url {
        tracing::info!(
            "[{}] Using canonical backend URL from port file: {}",
            log_prefix,
            url
        );
        return Ok(url);
    }

    tracing::info!(
        "[{}] Using port from port file: {}",
        log_prefix,
        port_info.main_port
    );
    let url = format!("http://{}:{}", host, port_info.main_port);
    tracing::info!("[{}] Using backend URL: {}", log_prefix, url);
    Ok(url)
}

fn init_process_logging(log_prefix: &str, version: &str) {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    sentry_utils::init_once(SentrySource::Mcp);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(EnvFilter::new("debug")),
        )
        .with(sentry_layer())
        .init();

    tracing::debug!(
        "[{}] Starting Vibe Kanban MCP server version {}...",
        log_prefix,
        version
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::middleware::from_fn;
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    use tokio::sync::oneshot;
    use utils::port_file::PortInfo;

    use super::{
        LaunchConfig, McpLaunchMode, McpServer, McpTransport, remap_session_not_found,
        resolve_base_url_from_sources, resolve_launch_config_from_iter,
    };

    #[test]
    fn orchestrator_mode_does_not_require_session_id() {
        let config = resolve_launch_config_from_iter(
            ["--mode".to_string(), "orchestrator".to_string()].into_iter(),
        )
        .expect("config should parse");

        assert_eq!(
            config,
            LaunchConfig {
                mode: McpLaunchMode::Orchestrator,
                transport: McpTransport::Stdio,
                host: None,
                backend_url: None,
            }
        );
    }

    #[test]
    fn session_id_flag_is_rejected() {
        let error = resolve_launch_config_from_iter(
            [
                "--mode".to_string(),
                "orchestrator".to_string(),
                "--session-id".to_string(),
                "x".to_string(),
            ]
            .into_iter(),
        )
        .expect_err("session id flag should be rejected");

        assert!(
            error
                .to_string()
                .contains("Unknown argument '--session-id'")
        );
    }

    #[test]
    fn vibe_backend_url_has_highest_precedence() {
        let url = resolve_base_url_from_sources(
            "test",
            Some("http://override:9999".to_string()),
            "legacy-host".to_string(),
            Some(7777),
            None,
        )
        .expect("base url should resolve");

        assert_eq!(url, "http://override:9999");
    }

    #[test]
    fn explicit_env_host_and_port_beat_port_file_url() {
        let url =
            resolve_base_url_from_sources("test", None, "env-host".to_string(), Some(7777), None)
                .expect("base url should resolve");

        assert_eq!(url, "http://env-host:7777");
    }

    #[test]
    fn canonical_backend_url_from_port_file_beats_legacy_reconstruction() {
        let url = resolve_base_url_from_sources(
            "test",
            None,
            "legacy-host".to_string(),
            None,
            Some(PortInfo {
                main_port: 4567,
                preview_proxy_port: Some(8901),
                backend_url: Some("http://localhost:4567".to_string()),
            }),
        )
        .expect("base url should resolve");

        assert_eq!(url, "http://localhost:4567");
    }

    #[test]
    fn legacy_port_file_still_reconstructs_from_host_and_port() {
        let url = resolve_base_url_from_sources(
            "test",
            None,
            "legacy-host".to_string(),
            None,
            Some(PortInfo {
                main_port: 4567,
                preview_proxy_port: Some(8901),
                backend_url: None,
            }),
        )
        .expect("base url should resolve");

        assert_eq!(url, "http://legacy-host:4567");
    }

    #[test]
    fn http_mode_with_port_parses() {
        let config = resolve_launch_config_from_iter(
            [
                "--http".to_string(),
                "--port".to_string(),
                "8765".to_string(),
                "--mode".to_string(),
                "global".to_string(),
            ]
            .into_iter(),
        )
        .expect("config should parse");

        assert_eq!(
            config,
            LaunchConfig {
                mode: McpLaunchMode::Global,
                transport: McpTransport::Http { port: 8765 },
                host: None,
                backend_url: None,
            }
        );
    }

    #[test]
    fn http_mode_requires_port_value() {
        let error = resolve_launch_config_from_iter(["--http".to_string()].into_iter())
            .expect_err("http mode without port should fail");

        assert!(error.to_string().contains("Missing value for --port"));
    }

    #[test]
    fn invalid_http_port_is_rejected() {
        let error = resolve_launch_config_from_iter(
            [
                "--http".to_string(),
                "--port".to_string(),
                "not-a-port".to_string(),
            ]
            .into_iter(),
        )
        .expect_err("invalid http port should fail");

        assert!(error.to_string().contains("Invalid value for --port"));
    }

    #[test]
    fn zero_http_port_is_rejected() {
        let error = resolve_launch_config_from_iter(
            ["--http".to_string(), "--port".to_string(), "0".to_string()].into_iter(),
        )
        .expect_err("zero http port should fail");

        assert!(error.to_string().contains("Expected 1-65535"));
    }

    #[tokio::test]
    async fn remap_session_not_found_returns_not_found() {
        let template_server = Arc::new(
            McpServer::new_global("http://127.0.0.1:9")
                .init()
                .await
                .expect("server should initialize without local context"),
        );
        let service: StreamableHttpService<McpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok((*template_server).clone()),
                Arc::new(LocalSessionManager::default()),
                StreamableHttpServerConfig::default().with_sse_keep_alive(None),
            );
        let router = axum::Router::new()
            .nest_service("/mcp", service)
            .layer(from_fn(remap_session_not_found));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("local addr should resolve");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/mcp"))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .header("mcp-session-id", "stale-session-id")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#)
            .send()
            .await
            .expect("request should complete");

        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

        let _ = shutdown_tx.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn http_server_initialize_responds_on_mcp_endpoint() {
        let template_server = Arc::new(
            McpServer::new_global("http://127.0.0.1:9")
                .init()
                .await
                .expect("server should initialize without local context"),
        );
        let service: StreamableHttpService<McpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok((*template_server).clone()),
                Arc::new(LocalSessionManager::default()),
                StreamableHttpServerConfig::default().with_sse_keep_alive(None),
            );
        let router = axum::Router::new()
            .nest_service("/mcp", service)
            .layer(from_fn(remap_session_not_found));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("local addr should resolve");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/mcp"))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .body(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#,
            )
            .send()
            .await
            .expect("request should complete");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body = response.text().await.expect("body should read");
        assert!(body.contains("\"protocolVersion\":\"2025-03-26\""));

        let _ = shutdown_tx.send(());
        let _ = server.await;
    }
}
