//! HTTP-to-stdio bridge implementation

use crate::error::{BridgeError, Result};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::ACCEPT},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
};
use dashmap::DashMap;
use futures::stream::StreamExt;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::{convert::Infallible, net::SocketAddr, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{broadcast, mpsc, oneshot},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info, warn};

/// Response format preference based on Accept header
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseFormat {
    /// Return JSON response (application/json)
    #[default]
    Json,
    /// Return SSE streaming response (text/event-stream)
    Sse,
}

/// Parse the Accept header to determine response format preference
///
/// Per R8A.1-R8A.7:
/// - If Accept contains only "text/event-stream", use SSE
/// - If Accept contains only "application/json", use JSON
/// - If Accept contains both, prefer JSON (backward compatibility)
/// - If no Accept header, default to JSON
fn parse_accept_header(headers: &HeaderMap) -> ResponseFormat {
    let accept = headers
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let accepts_json = accept.contains("application/json") || accept.contains("*/*");
    let accepts_sse = accept.contains("text/event-stream");

    match (accepts_json, accepts_sse) {
        (true, _) => ResponseFormat::Json, // JSON takes priority when both present
        (false, true) => ResponseFormat::Sse,
        (false, false) => ResponseFormat::Json, // Default to JSON
    }
}

/// Configuration for the HTTP bridge
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Address to bind the HTTP server to
    pub bind_addr: SocketAddr,
    /// Command to run the MCP server
    pub server_command: String,
    /// Arguments to pass to the MCP server
    pub server_args: Vec<String>,
    /// Enable colored terminal output for STDIN/STDOUT/STDERR (debug mode only)
    pub enable_colored_output: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            server_command: "ahma_mcp".to_string(),
            server_args: vec![],
            enable_colored_output: false,
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

        let stderr_mode = if config.enable_colored_output {
            Stdio::piped()
        } else {
            Stdio::inherit()
        };

        let mut child = match Command::new(&config.server_command)
            .args(&config.server_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr_mode)
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

        // Handle stderr if colored output is enabled
        let mut stderr_reader = if config.enable_colored_output {
            child
                .stderr
                .take()
                .map(|stderr| BufReader::new(stderr).lines())
        } else {
            None
        };

        loop {
            tokio::select! {
                // Handle outgoing messages (HTTP -> Stdio)
                Some(msg) = rx.recv() => {
                    if let Ok(json_str) = serde_json::to_string(&msg) {
                        debug!("Sending to MCP server: {}", json_str);

                        // Echo STDIN in cyan if colored output is enabled
                        if config.enable_colored_output {
                            eprintln!("{}", format!("→ STDIN: {}", json_str).cyan());
                        }

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

                    // Echo STDOUT in green if colored output is enabled
                    if config.enable_colored_output {
                        eprintln!("{}", format!("← STDOUT: {}", line).green());
                    }

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

                // Handle stderr if colored output is enabled
                result = async {
                    if let Some(ref mut reader) = stderr_reader {
                        reader.next_line().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match result {
                        Ok(Some(line)) => {
                            if !line.is_empty() {
                                // Echo STDERR in red
                                eprintln!("{}", format!("⚠ STDERR: {}", line).red());
                            }
                        }
                        Ok(None) => {
                            // stderr closed
                        }
                        Err(e) => {
                            error!("Failed to read stderr: {}", e);
                        }
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

    let initial_event = axum::response::sse::Event::default()
        .event("endpoint")
        .data("/mcp");

    let stream = tokio_stream::StreamExt::chain(tokio_stream::once(Ok(initial_event)), stream);

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Create a JSON response with appropriate headers
fn json_response(value: Value) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&value).unwrap_or_default(),
        ))
        .unwrap_or_else(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create response",
            )
                .into_response()
        })
}

/// Create an SSE response for a single JSON-RPC response
fn sse_single_response(value: Value) -> Response {
    let json_str = serde_json::to_string(&value).unwrap_or_default();
    let stream = futures::stream::once(async move {
        Ok::<_, Infallible>(axum::response::sse::Event::default().data(json_str))
    });
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

/// Create an error response in the appropriate format
fn error_response(format: ResponseFormat, code: i32, message: &str) -> Response {
    let error_json = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message
        }
    });

    match format {
        ResponseFormat::Json => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&error_json).unwrap_or_default(),
            ))
            .unwrap_or_else(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to create response",
                )
                    .into_response()
            }),
        ResponseFormat::Sse => {
            let json_str = serde_json::to_string(&error_json).unwrap_or_default();
            let stream = futures::stream::once(async move {
                Ok::<_, Infallible>(axum::response::sse::Event::default().data(json_str))
            });
            Sse::new(stream).into_response()
        }
    }
}

/// Handle MCP JSON-RPC requests with content negotiation
///
/// Supports both JSON and SSE response formats based on Accept header (R8A)
async fn handle_mcp_request(
    State(state): State<Arc<BridgeState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    debug!("Received HTTP request");

    // Determine response format from Accept header (R8A.1)
    let response_format = parse_accept_header(&headers);
    debug!("Response format: {:?}", response_format);

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
        return error_response(
            response_format,
            -32603,
            "Failed to send request to server process",
        );
    }

    // If we expect a response, wait for it
    if let Some(rx) = response_rx {
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(response)) => {
                // Return response in the requested format (R8A.4, R8A.6)
                match response_format {
                    ResponseFormat::Json => json_response(response),
                    ResponseFormat::Sse => sse_single_response(response),
                }
            }
            Ok(Err(_)) => error_response(
                response_format,
                -32603,
                "Channel closed before response received",
            ),
            Err(_) => {
                if let Some(id) = &id_opt {
                    state.pending_requests.remove(id);
                }
                // Timeout error response
                let error_json = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32000,
                        "message": "Request timed out"
                    }
                });
                match response_format {
                    ResponseFormat::Json => Response::builder()
                        .status(StatusCode::GATEWAY_TIMEOUT)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(
                            serde_json::to_vec(&error_json).unwrap_or_default(),
                        ))
                        .unwrap_or_else(|_| {
                            (StatusCode::GATEWAY_TIMEOUT, "Request timed out").into_response()
                        }),
                    ResponseFormat::Sse => {
                        let json_str = serde_json::to_string(&error_json).unwrap_or_default();
                        let stream = futures::stream::once(async move {
                            Ok::<_, Infallible>(
                                axum::response::sse::Event::default().data(json_str),
                            )
                        });
                        Sse::new(stream).into_response()
                    }
                }
            }
        }
    } else {
        // Notification, return success immediately
        let success_json = serde_json::json!({"jsonrpc": "2.0", "result": null});
        match response_format {
            ResponseFormat::Json => json_response(success_json),
            ResponseFormat::Sse => sse_single_response(success_json),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    // ==================== parse_accept_header unit tests ====================

    #[test]
    fn test_parse_accept_header_json_only() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Json);
    }

    #[test]
    fn test_parse_accept_header_sse_only() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "text/event-stream".parse().unwrap());
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Sse);
    }

    #[test]
    fn test_parse_accept_header_both_json_first() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            "application/json, text/event-stream".parse().unwrap(),
        );
        // JSON should take priority when both present
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Json);
    }

    #[test]
    fn test_parse_accept_header_both_sse_first() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            "text/event-stream, application/json".parse().unwrap(),
        );
        // JSON should still take priority regardless of order
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Json);
    }

    #[test]
    fn test_parse_accept_header_no_header() {
        let headers = HeaderMap::new();
        // Default to JSON when no Accept header
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Json);
    }

    #[test]
    fn test_parse_accept_header_wildcard() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "*/*".parse().unwrap());
        // Wildcard implies JSON acceptance
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Json);
    }

    #[test]
    fn test_parse_accept_header_unknown_type() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "text/html".parse().unwrap());
        // Unknown type defaults to JSON
        assert_eq!(parse_accept_header(&headers), ResponseFormat::Json);
    }

    #[test]
    fn test_response_format_default() {
        assert_eq!(ResponseFormat::default(), ResponseFormat::Json);
    }

    // ==================== Original tests ====================

    #[test]
    fn test_default_config() {
        let config = BridgeConfig::default();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:3000");
        assert_eq!(config.server_command, "ahma_mcp");
        assert!(config.server_args.is_empty());
    }

    #[test]
    fn test_config_with_custom_values() {
        let config = BridgeConfig {
            bind_addr: "0.0.0.0:8080".parse().unwrap(),
            server_command: "custom_server".to_string(),
            server_args: vec!["--arg1".to_string(), "value1".to_string()],
            enable_colored_output: false,
        };
        assert_eq!(config.bind_addr.to_string(), "0.0.0.0:8080");
        assert_eq!(config.server_command, "custom_server");
        assert_eq!(config.server_args.len(), 2);
        assert_eq!(config.server_args[0], "--arg1");
        assert_eq!(config.server_args[1], "value1");
    }

    #[test]
    fn test_config_clone() {
        let config = BridgeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.bind_addr, cloned.bind_addr);
        assert_eq!(config.server_command, cloned.server_command);
        assert_eq!(config.server_args, cloned.server_args);
    }

    #[test]
    fn test_config_debug() {
        let config = BridgeConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("BridgeConfig"));
        assert!(debug_str.contains("127.0.0.1:3000"));
        assert!(debug_str.contains("ahma_mcp"));
    }

    fn create_test_state() -> Arc<BridgeState> {
        let (tx, _rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests,
        })
    }

    fn create_app(state: Arc<BridgeState>) -> Router {
        Router::new()
            .route("/health", get(health_check))
            .route("/mcp", post(handle_mcp_request))
            .route("/sse", get(handle_sse))
            .layer(CorsLayer::permissive())
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_check_endpoint() {
        let state = create_test_state();
        let app = create_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(&body[..], b"OK");
    }

    #[tokio::test]
    async fn test_mcp_notification_no_id() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests,
        });
        let app = create_app(state);

        // Spawn a task to consume the notification (otherwise channel fills up)
        tokio::spawn(async move {
            while let Some(_msg) = rx.recv().await {
                // Just consume the message
            }
        });

        // JSON-RPC notification (no "id" field) should return immediately
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"notifications/test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert!(json["result"].is_null());
    }

    #[tokio::test]
    async fn test_mcp_request_with_string_id() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                // Verify the message was received
                assert_eq!(msg["method"], "test/method");
                // Get the request ID and send a response
                let id = msg["id"].as_str().unwrap().to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"success": true}
                    }));
                }
            }
        });

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"test-123"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], "test-123");
        assert_eq!(json["result"]["success"], true);
    }

    #[tokio::test]
    async fn test_mcp_request_with_numeric_id() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                // Numeric IDs are serialized as strings internally
                let id = msg["id"].to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 42,
                        "result": {"numeric_id": true}
                    }));
                }
            }
        });

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":42}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], 42);
        assert_eq!(json["result"]["numeric_id"], true);
    }

    #[tokio::test]
    async fn test_mcp_request_channel_closed() {
        // Create state with closed sender
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // Close the receiver immediately
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests,
        });
        let app = create_app(state);

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"fail-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], -32603);
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Failed to send request")
        );
    }

    #[tokio::test]
    async fn test_mcp_request_response_channel_closed() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task that receives but drops the sender without sending
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                let id = msg["id"].as_str().unwrap().to_string();
                // Remove and drop the sender without sending a response
                pending_clone.remove(&id);
            }
        });

        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"drop-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], -32603);
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Channel closed")
        );
    }

    #[tokio::test]
    async fn test_sse_endpoint_returns_initial_event() {
        let state = create_test_state();
        let app = create_app(state);

        let request = Request::builder()
            .uri("/sse")
            .header("accept", "text/event-stream")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        // SSE returns a stream, so we just verify it starts correctly
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""));
        assert!(content_type.unwrap_or("").contains("text/event-stream"));
    }

    #[tokio::test]
    async fn test_sse_broadcasts_messages() {
        let (tx, _rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());

        // Clone broadcast_tx to send a message
        let broadcast_tx_clone = broadcast_tx.clone();

        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests,
        });

        // Subscribe before creating the app
        let mut subscriber = state.broadcast_tx.subscribe();

        // Send a broadcast message
        let _ = broadcast_tx_clone.send(r#"{"test":"message"}"#.to_string());

        // Verify the message was received
        let received = subscriber.recv().await.unwrap();
        assert_eq!(received, r#"{"test":"message"}"#);
    }

    #[test]
    fn test_pending_requests_insert_and_remove() {
        let pending: DashMap<String, oneshot::Sender<Value>> = DashMap::new();
        let (tx, _rx) = oneshot::channel();

        pending.insert("test-id".to_string(), tx);
        assert!(pending.contains_key("test-id"));

        let removed = pending.remove("test-id");
        assert!(removed.is_some());
        assert!(!pending.contains_key("test-id"));
    }

    #[test]
    fn test_pending_requests_clear() {
        let pending: DashMap<String, oneshot::Sender<Value>> = DashMap::new();
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();

        pending.insert("id1".to_string(), tx1);
        pending.insert("id2".to_string(), tx2);
        assert_eq!(pending.len(), 2);

        pending.clear();
        assert!(pending.is_empty());
    }

    // ==================== Streaming Response Tests (R8A) ====================

    #[tokio::test]
    async fn test_mcp_request_with_accept_json_returns_json() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                let id = msg["id"].as_str().unwrap().to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"format": "json"}
                    }));
                }
            }
        });

        // Request with explicit Accept: application/json header
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"json-accept-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""));
        assert!(
            content_type.unwrap_or("").contains("application/json"),
            "Expected application/json content type"
        );

        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["result"]["format"], "json");
    }

    #[tokio::test]
    async fn test_mcp_request_with_accept_sse_returns_sse() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                let id = msg["id"].as_str().unwrap().to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"format": "sse"}
                    }));
                }
            }
        });

        // Request with Accept: text/event-stream header
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"sse-accept-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""));
        assert!(
            content_type.unwrap_or("").contains("text/event-stream"),
            "Expected text/event-stream content type, got: {:?}",
            content_type
        );
    }

    #[tokio::test]
    async fn test_mcp_request_with_accept_both_prefers_json() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                let id = msg["id"].as_str().unwrap().to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"format": "json_preferred"}
                    }));
                }
            }
        });

        // Request with both Accept headers (JSON first = higher priority per HTTP spec)
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"both-accept-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""));
        // When both are accepted, JSON should be preferred (backward compatibility)
        assert!(
            content_type.unwrap_or("").contains("application/json"),
            "Expected application/json when both are accepted"
        );
    }

    #[tokio::test]
    async fn test_mcp_request_without_accept_defaults_to_json() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                let id = msg["id"].as_str().unwrap().to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"format": "default"}
                    }));
                }
            }
        });

        // Request without Accept header - should default to JSON
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"no-accept-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""));
        assert!(
            content_type.unwrap_or("").contains("application/json"),
            "Expected application/json as default when no Accept header"
        );
    }

    #[tokio::test]
    async fn test_mcp_request_sse_only_accept() {
        let (tx, mut rx) = mpsc::channel(100);
        let (broadcast_tx, _) = broadcast::channel(100);
        let pending_requests = Arc::new(DashMap::new());
        let state = Arc::new(BridgeState {
            sender: tx,
            broadcast_tx,
            pending_requests: pending_requests.clone(),
        });
        let app = create_app(state);

        // Spawn a task to handle the request and send a response
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Some(msg) = rx.recv().await {
                let id = msg["id"].as_str().unwrap().to_string();
                if let Some((_, sender)) = pending_clone.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"format": "sse_only"}
                    }));
                }
            }
        });

        // Request with Accept: text/event-stream only
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","method":"test/method","id":"sse-only-test"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""));
        assert!(
            content_type.unwrap_or("").contains("text/event-stream"),
            "Expected text/event-stream when Accept is SSE only"
        );
    }
}
