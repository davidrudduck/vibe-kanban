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
    port_file::read_port_file,
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
struct LaunchConfig {
    mode: McpLaunchMode,
    transport: McpTransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum McpTransport {
    Stdio,
    Http { port: u16 },
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

            let base_url = resolve_base_url("vibe-kanban-mcp").await?;
            let LaunchConfig { mode, transport } = launch_config;

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
                port = Some(parsed_port);
            }
            "-h" | "--help" => {
                println!(
                    "Usage: vibe-kanban-mcp [--mode <global|orchestrator>] [--http --port <port>]"
                );
                std::process::exit(0);
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown argument '{arg}'. Usage: vibe-kanban-mcp [--mode <global|orchestrator>] [--http --port <port>]"
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

    Ok(LaunchConfig { mode, transport })
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

async fn resolve_base_url(log_prefix: &str) -> anyhow::Result<String> {
    if let Ok(url) = std::env::var("VIBE_BACKEND_URL") {
        tracing::info!(
            "[{}] Using backend URL from VIBE_BACKEND_URL: {}",
            log_prefix,
            url
        );
        return Ok(url);
    }

    let host = std::env::var(HOST_ENV)
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let port = match std::env::var(PORT_ENV)
        .or_else(|_| std::env::var("BACKEND_PORT"))
        .or_else(|_| std::env::var("PORT"))
    {
        Ok(port_str) => {
            tracing::info!("[{}] Using port from environment: {}", log_prefix, port_str);
            port_str
                .parse::<u16>()
                .map_err(|error| anyhow::anyhow!("Invalid port value '{}': {}", port_str, error))?
        }
        Err(_) => {
            let port = read_port_file("vibe-kanban").await?;
            tracing::info!("[{}] Using port from port file: {}", log_prefix, port);
            port
        }
    };

    let url = format!("http://{}:{}", host, port);
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

    use super::{
        LaunchConfig, McpLaunchMode, McpServer, McpTransport, remap_session_not_found,
        resolve_launch_config_from_iter,
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
