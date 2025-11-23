//! HTTP-to-stdio bridge implementation

use crate::error::{BridgeError, Result};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
};
use dashmap::DashMap;
use futures::stream::StreamExt;
use serde_json::Value;
use std::{convert::Infallible, net::SocketAddr, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{broadcast, mpsc, oneshot},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info, warn};

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
    /// Channel to send messages to the MCP process manager
    sender: mpsc::Sender<Value>,
    /// Broadcast channel for SSE events (notifications)
    broadcast_tx: broadcast::Sender<String>,
    /// Map of request IDs to response channels
    pending_requests: Arc<DashMap<String, oneshot::Sender<Value>>>,
}

/// Start the HTTP bridge server
pub async fn start_bridge(config: BridgeConfig) -> Result<()> {
    info!("Starting HTTP bridge on {}", config.bind_addr);

    // Channels
    let (tx, rx) = mpsc::channel(100);
    let (broadcast_tx, _) = broadcast::channel(100);
    let pending_requests = Arc::new(DashMap::new());

    // Start the process manager
    let manager_broadcast = broadcast_tx.clone();
    let manager_pending = pending_requests.clone();
    let manager_config = config.clone();

    tokio::spawn(async move {
        manage_process(manager_config, rx, manager_broadcast, manager_pending).await;
    });

    let state = Arc::new(BridgeState {
        sender: tx,
        broadcast_tx,
        pending_requests,
    });

    // Build the router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_request))
        .route("/sse", get(handle_sse))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!("HTTP bridge listening on http://{}", config.bind_addr);
    info!("POST JSON-RPC messages to http://{}/mcp", config.bind_addr);
    info!("SSE endpoint at http://{}/sse", config.bind_addr);

    // Start the server
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .map_err(|e| BridgeError::HttpServer(format!("Failed to bind: {}", e)))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| BridgeError::HttpServer(format!("Server error: {}", e)))?;

    Ok(())
}

async fn manage_process(
    config: BridgeConfig,
    mut rx: mpsc::Receiver<Value>,
    broadcast_tx: broadcast::Sender<String>,
    pending_requests: Arc<DashMap<String, oneshot::Sender<Value>>>,
) {
    loop {
        info!(
            "Spawning MCP server: {} {}",
            config.server_command,
            config.server_args.join(" ")
        );

        let mut child = match Command::new(&config.server_command)
            .args(&config.server_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn MCP server: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let mut stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let mut stdout_reader = BufReader::new(stdout).lines();

        loop {
            tokio::select! {
                // Handle outgoing messages (HTTP -> Stdio)
                Some(msg) = rx.recv() => {
                    if let Ok(json_str) = serde_json::to_string(&msg) {
                        debug!("Sending to MCP server: {}", json_str);
                        if let Err(e) = stdin.write_all(json_str.as_bytes()).await {
                            error!("Failed to write to stdin: {}", e);
                            break;
                        }
                        if let Err(e) = stdin.write_all(b"\n").await {
                            error!("Failed to write newline to stdin: {}", e);
                            break;
                        }
                        if let Err(e) = stdin.flush().await {
                            error!("Failed to flush stdin: {}", e);
                            break;
                        }
                    }
                }

                // Handle incoming messages (Stdio -> HTTP/SSE)
                Ok(Some(line)) = stdout_reader.next_line() => {
                    if line.is_empty() { continue; }
                    debug!("Received from MCP server: {}", line);

                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        // Check if it's a response to a pending request
                        if let Some(id) = value.get("id") {
                            let id_str = if id.is_string() {
                                id.as_str().unwrap().to_string()
                            } else {
                                id.to_string()
                            };

                            if let Some((_, sender)) = pending_requests.remove(&id_str) {
                                let _ = sender.send(value);
                                continue;
                            }
                        }

                        // If not a response, or ID not found, treat as notification/event
                        // Broadcast to SSE clients
                        let _ = broadcast_tx.send(line);
                    } else {
                        warn!("Failed to parse JSON from server: {}", line);
                    }
                }

                // Process exit
                _ = child.wait() => {
                    warn!("MCP server process exited");
                    break;
                }
            }
        }

        // Clean up pending requests on crash
        pending_requests.clear();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Handle SSE connections
async fn handle_sse(State(state): State<Arc<BridgeState>>) -> impl IntoResponse {
    let rx = state.broadcast_tx.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(
        |msg| -> std::result::Result<_, Infallible> {
            match msg {
                Ok(json_str) => Ok(axum::response::sse::Event::default().data(json_str)),
                Err(_) => Ok(axum::response::sse::Event::default().comment("missed messages")),
            }
        },
    );

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Handle MCP JSON-RPC requests
async fn handle_mcp_request(
    State(state): State<Arc<BridgeState>>,
    Json(payload): Json<Value>,
) -> Response {
    debug!("Received HTTP request");

    // If the request has an ID, we expect a response
    let id_opt = payload.get("id").map(|id| {
        if id.is_string() {
            id.as_str().unwrap().to_string()
        } else {
            id.to_string()
        }
    });

    let (response_tx, response_rx) = if id_opt.is_some() {
        let (tx, rx) = oneshot::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    if let Some(id) = &id_opt {
        state
            .pending_requests
            .insert(id.clone(), response_tx.unwrap());
    }

    // Send to process
    if let Err(e) = state.sender.send(payload).await {
        error!("Failed to send request to server process: {}", e);
        if let Some(id) = &id_opt {
            state.pending_requests.remove(id);
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32603,
                    "message": "Failed to send request to server process"
                }
            })),
        )
            .into_response();
    }

    // If we expect a response, wait for it
    if let Some(rx) = response_rx {
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(response)) => Json(response).into_response(),
            Ok(Err(_)) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32603,
                        "message": "Channel closed before response received"
                    }
                })),
            )
                .into_response(),
            Err(_) => {
                if let Some(id) = &id_opt {
                    state.pending_requests.remove(id);
                }
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {
                            "code": -32000,
                            "message": "Request timed out"
                        }
                    })),
                )
                    .into_response()
            }
        }
    } else {
        // Notification, return success immediately
        (
            StatusCode::OK,
            Json(serde_json::json!({"jsonrpc": "2.0", "result": null})),
        )
            .into_response()
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
