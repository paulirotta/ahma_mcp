//! HTTP-to-stdio bridge implementation

use crate::error::{BridgeError, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use std::{net::SocketAddr, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, info, warn};

/// Configuration for the HTTP bridge
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Address to bind the HTTP server to
    pub bind_addr: SocketAddr,
    /// Command to run the MCP server
    pub server_command: String,
    /// Arguments to pass to the MCP server
    pub server_args: Vec<String>,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            server_command: "ahma_mcp".to_string(),
            server_args: vec![],
        }
    }
}

/// Shared state for the bridge
struct BridgeState {
    process: Arc<Mutex<Option<McpProcess>>>,
    config: BridgeConfig,
}

/// Wrapper for the MCP server process
struct McpProcess {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl McpProcess {
    /// Spawn a new MCP server process
    async fn spawn(config: &BridgeConfig) -> Result<Self> {
        info!(
            "Spawning MCP server: {} {}",
            config.server_command,
            config.server_args.join(" ")
        );

        let mut child = Command::new(&config.server_command)
            .args(&config.server_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                BridgeError::ServerProcess(format!("Failed to spawn server: {}", e))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            BridgeError::ServerProcess("Failed to get stdin handle".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            BridgeError::ServerProcess("Failed to get stdout handle".to_string())
        })?;

        let stdout = BufReader::new(stdout);

        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    /// Send a JSON-RPC message to the server
    async fn send(&mut self, message: &Value) -> Result<()> {
        let json_str = serde_json::to_string(message)?;
        debug!("Sending to MCP server: {}", json_str);

        self.stdin
            .write_all(json_str.as_bytes())
            .await
            .map_err(|e| BridgeError::Communication(format!("Failed to write: {}", e)))?;

        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| BridgeError::Communication(format!("Failed to write newline: {}", e)))?;

        self.stdin
            .flush()
            .await
            .map_err(|e| BridgeError::Communication(format!("Failed to flush: {}", e)))?;

        Ok(())
    }

    /// Receive a JSON-RPC message from the server
    async fn receive(&mut self) -> Result<Value> {
        let mut line = String::new();

        self.stdout
            .read_line(&mut line)
            .await
            .map_err(|e| BridgeError::Communication(format!("Failed to read: {}", e)))?;

        if line.is_empty() {
            return Err(BridgeError::Communication(
                "Server closed connection".to_string(),
            ));
        }

        debug!("Received from MCP server: {}", line.trim());

        let value: Value = serde_json::from_str(&line)?;
        Ok(value)
    }

    /// Check if the process is still running
    fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        if self.is_alive() {
            info!("Terminating MCP server process");
            let _ = self.child.start_kill();
        }
    }
}

/// Start the HTTP bridge server
pub async fn start_bridge(config: BridgeConfig) -> Result<()> {
    info!("Starting HTTP bridge on {}", config.bind_addr);

    let state = BridgeState {
        process: Arc::new(Mutex::new(None)),
        config: config.clone(),
    };

    // Build the router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_request))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state));

    info!("HTTP bridge listening on http://{}", config.bind_addr);
    info!("POST JSON-RPC messages to http://{}/mcp", config.bind_addr);

    // Start the server
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .map_err(|e| BridgeError::HttpServer(format!("Failed to bind: {}", e)))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| BridgeError::HttpServer(format!("Server error: {}", e)))?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Handle MCP JSON-RPC requests
async fn handle_mcp_request(
    State(state): State<Arc<BridgeState>>,
    Json(payload): Json<Value>,
) -> Response {
    debug!("Received HTTP request");

    // Get or create the MCP process
    let mut process_guard = state.process.lock().await;

    if process_guard.is_none() || !process_guard.as_mut().unwrap().is_alive() {
        if process_guard.is_some() {
            warn!("MCP server process died, restarting...");
        }

        match McpProcess::spawn(&state.config).await {
            Ok(process) => {
                *process_guard = Some(process);
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {
                            "code": -32603,
                            "message": format!("Failed to spawn MCP server: {}", e)
                        }
                    }))
                ).into_response();
            }
        }
    }

    let process = process_guard.as_mut().unwrap();

    // Send the request to the MCP server
    if let Err(e) = process.send(&payload).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32603,
                    "message": format!("Failed to send request: {}", e)
                }
            }))
        ).into_response();
    }

    // Receive the response from the MCP server
    match process.receive().await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32603,
                    "message": format!("Failed to receive response: {}", e)
                }
            }))
        ).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BridgeConfig::default();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:3000");
        assert_eq!(config.server_command, "ahma_mcp");
        assert!(config.server_args.is_empty());
    }
}

