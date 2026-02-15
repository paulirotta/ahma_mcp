//! HTTP-to-stdio bridge implementation

use crate::error::{BridgeError, Result};
use crate::session::{DEFAULT_HANDSHAKE_TIMEOUT_SECS, SessionManager, SessionManagerConfig};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::stream::StreamExt;
use serde_json::Value;
use std::{convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{debug, error, info, warn};

/// Configuration for the HTTP bridge server.
///
/// Use `Default` to get a baseline configuration or construct manually for full control.
///
/// # Example
///
/// ```rust
/// use ahma_http_bridge::BridgeConfig;
/// use std::path::PathBuf;
///
/// let config = BridgeConfig {
///     bind_addr: "0.0.0.0:8080".parse().unwrap(),
///     server_command: "/usr/local/bin/my-mcp-server".into(),
///     server_args: vec!["--verbose".into()],
///     // Optional explicit fallback scope for clients without roots support
///     default_sandbox_scope: Some(PathBuf::from("/tmp/sandbox")),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// local address to bind the HTTP server to (e.g., `127.0.0.1:3000`).
    /// Use port 0 to bind to a random available port.
    pub bind_addr: SocketAddr,

    /// Path or command name of the MCP server executable to spawn.
    /// This command will be executed as a subprocess for each session.
    pub server_command: String,

    /// Command-line arguments to pass to the MCP server.
    pub server_args: Vec<String>,

    /// If true, preserves ANSI color codes in the subprocess output (useful for debugging).
    /// If false, colors are stripped or disabled depending on the subprocess behavior.
    pub enable_colored_output: bool,

    /// Explicit fallback sandbox directory for clients that do not provide workspace roots.
    ///
    /// If `None`, clients must provide roots/list to complete handshake and unlock tools.
    pub default_sandbox_scope: Option<PathBuf>,

    /// Timeout in seconds for the MCP handshake to complete.
    /// If the handshake (SSE connection + roots/list response) doesn't complete
    /// within this time, tool calls will return a timeout error.
    /// Defaults to 45 seconds.
    pub handshake_timeout_secs: u64,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            server_command: "ahma_mcp".to_string(),
            server_args: vec![],
            enable_colored_output: false,
            default_sandbox_scope: None,
            handshake_timeout_secs: DEFAULT_HANDSHAKE_TIMEOUT_SECS,
        }
    }
}

/// Shared state for the bridge
struct BridgeState {
    /// Session manager (session isolation mode only)
    session_manager: Arc<SessionManager>,
}

/// MCP Session-Id header name (per MCP spec 2025-03-26)
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Starts the HTTP bridge server and blocks until shutdown.
///
/// This function initializes the session manager, sets up the Axum router for MCP
/// endpoints, and binds to the specified address.
///
/// # Returns
///
/// * `Ok(())` upon graceful shutdown (currently runs indefinitely).
/// * `Err(BridgeError)` if binding fails or the server encounters a fatal error.
///
/// # Port Binding
///
/// If `config.bind_addr` specifies port 0, the OS will assign a random available port.
/// The actual bound port is printed to stderr as `AHMA_BOUND_PORT=<port>` to assist
/// with test infrastructure integration.
///
/// # Example
///
/// ```rust,no_run
/// use ahma_http_bridge::{BridgeConfig, start_bridge};
///
/// #[tokio::main]
/// async fn main() {
///    let config = BridgeConfig::default();
///    if let Err(e) = start_bridge(config).await {
///        eprintln!("Bridge failed: {}", e);
///    }
/// }
/// ```
pub async fn start_bridge(config: BridgeConfig) -> Result<()> {
    info!("Starting HTTP bridge on {}", config.bind_addr);

    info!("Session isolation: ENABLED (always-on)");
    let session_config = SessionManagerConfig {
        server_command: config.server_command.clone(),
        server_args: config.server_args.clone(),
        default_scope: config.default_sandbox_scope.clone(),
        enable_colored_output: config.enable_colored_output,
        handshake_timeout_secs: config.handshake_timeout_secs,
    };
    let session_manager = Arc::new(SessionManager::new(session_config));
    let state = Arc::new(BridgeState { session_manager });

    // Build the router
    // MCP Streamable HTTP transport: single endpoint supporting POST (requests), GET (SSE), DELETE (terminate)
    // See: https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#streamable-http
    let app = Router::new()
        .route("/health", get(health_check))
        .route(
            "/mcp",
            post(handle_mcp_request)
                .get(handle_sse_stream)
                .delete(handle_session_delete),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start the server - bind first to get actual port (important when port 0 is used)
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .map_err(|e| BridgeError::HttpServer(format!("Failed to bind: {}", e)))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| BridgeError::HttpServer(format!("Failed to get local addr: {}", e)))?;

    info!("HTTP bridge listening on http://{}", local_addr);
    info!("MCP endpoint (POST): http://{}/mcp", local_addr);
    info!("MCP endpoint (GET/SSE): http://{}/mcp", local_addr);

    // Print machine-readable bound port for test infrastructure (always print, tests parse it)
    eprintln!("AHMA_BOUND_PORT={}", local_addr.port());

    axum::serve(listener, app)
        .await
        .map_err(|e| BridgeError::HttpServer(format!("Server error: {}", e)))?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Handle DELETE requests to terminate a session (R8.4.7)
///
/// Per MCP specification: HTTP DELETE with `Mcp-Session-Id` header terminates
/// the session and its subprocess.
///
/// # Example
///
/// ```bash
/// curl -X DELETE http://localhost:3000/mcp \
///   -H "mcp-session-id: <session-uuid>"
/// ```
///
/// # Returns
///
/// - 204 No Content on successful termination
/// - 400 Bad Request if session ID header is missing
/// - 404 Not Found if session doesn't exist
async fn handle_session_delete(
    State(state): State<Arc<BridgeState>>,
    headers: HeaderMap,
) -> Response {
    // Get session ID from header
    let session_id = match headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(id) => id.to_string(),
        None => {
            warn!("DELETE request without session ID header");
            return (StatusCode::BAD_REQUEST, "Missing Mcp-Session-Id header").into_response();
        }
    };

    info!(session_id = %session_id, "Session termination requested via HTTP DELETE");

    // Check if session exists before terminating
    if !state.session_manager.session_exists(&session_id) {
        debug!(session_id = %session_id, "Session not found for DELETE request");
        return StatusCode::NOT_FOUND.into_response();
    }

    // Terminate the session
    match state
        .session_manager
        .terminate_session(
            &session_id,
            crate::session::SessionTerminationReason::ClientRequested,
        )
        .await
    {
        Ok(()) => {
            info!(session_id = %session_id, "Session terminated successfully");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            error!(session_id = %session_id, "Failed to terminate session: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to terminate session: {}", e),
            )
                .into_response()
        }
    }
}

/// Handle SSE stream connections for server-to-client messages
///
/// This enables the MCP Streamable HTTP transport pattern where:
/// - POST /mcp sends client→server requests (existing)
/// - GET /mcp opens SSE stream for server→client messages (this handler)
///
/// The server uses SSE to:
/// 1. Send `roots/list` requests to discover client workspace folders
/// 2. Send notifications (if any)
/// 3. Send requests that need client responses
///
/// Note: Returns 404 (not 400/501) when session ID is missing or invalid. This prevents
/// clients from detecting SSE support during initial probing, avoiding OAuth prompts
/// for servers that don't require authentication.
///
/// # client Example
///
/// Clients should open this stream immediately after receiving a Session ID.
///
/// ```javascript
/// const eventSource = new EventSource("http://localhost:3000/mcp", {
///     headers: { "mcp-session-id": sessionId }
/// });
/// eventSource.onmessage = (event) => {
///     const msg = JSON.parse(event.data);
///     console.log("Received:", msg);
/// };
/// ```
async fn handle_sse_stream(State(state): State<Arc<BridgeState>>, headers: HeaderMap) -> Response {
    // Get session ID from header - required for SSE
    // Return 404 (not 400) to hide SSE from clients without a session
    let session_id = match headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(id) => id.to_string(),
        None => {
            debug!("SSE request without session ID header - returning 404");
            // 404 makes clients think SSE doesn't exist, avoiding OAuth probes
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    debug!(session_id = %session_id, "SSE GET request received with session header");

    // Get the session - 404 if not found
    let session = match state.session_manager.get_session(&session_id) {
        Some(s) => s,
        None => {
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    // Check if session is terminated
    if session.is_terminated() {
        return StatusCode::NOT_FOUND.into_response();
    }

    info!(session_id = %session_id, "SSE stream opened");

    // Subscribe to the session's broadcast channel
    let rx = session.subscribe();

    // Mark SSE as connected - if MCP is already initialized, this will trigger roots/list_changed
    if let Err(e) = session.mark_sse_connected().await {
        warn!(session_id = %session_id, "Failed to mark SSE connected: {}", e);
    }

    // Convert broadcast receiver to a stream of SSE events
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let session_id = session_id.clone();
        async move {
            match result {
                Ok(msg) => {
                    debug!(session_id = %session_id, "Sending SSE event: {}", msg);
                    Some(Ok::<_, Infallible>(Event::default().data(msg)))
                }
                Err(e) => {
                    warn!(session_id = %session_id, "Broadcast receive error: {}", e);
                    // Skip errors (lagged receiver)
                    None
                }
            }
        }
    });

    // Return SSE response with keep-alive
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Handle MCP JSON-RPC requests with content negotiation
///
/// Supports both JSON and SSE response formats based on Accept header (R8A)
/// In session isolation mode, routes requests to the correct session subprocess
async fn handle_mcp_request(
    State(state): State<Arc<BridgeState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    debug!("Received HTTP request");

    crate::request_handler::handle_session_isolated_request(
        state.session_manager.clone(),
        headers,
        payload,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::fs;
    use tempfile::TempDir;
    use tower::ServiceExt;

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
            default_sandbox_scope: Some(PathBuf::from("/tmp")),
            handshake_timeout_secs: 10,
        };
        assert_eq!(config.bind_addr.to_string(), "0.0.0.0:8080");
        assert_eq!(config.server_command, "custom_server");
        assert_eq!(config.server_args.len(), 2);
        assert_eq!(config.server_args[0], "--arg1");
        assert_eq!(config.server_args[1], "value1");
        assert_eq!(config.handshake_timeout_secs, 10);
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

    fn create_app(state: Arc<BridgeState>) -> Router {
        Router::new()
            .route("/health", get(health_check))
            .route("/mcp", post(handle_mcp_request))
            .layer(CorsLayer::permissive())
            .with_state(state)
    }

    fn create_state_with_session_manager(session_manager: Arc<SessionManager>) -> Arc<BridgeState> {
        Arc::new(BridgeState { session_manager })
    }

    fn write_mock_mcp_server_script(temp_dir: &TempDir) -> std::path::PathBuf {
        let script_path = temp_dir.path().join("mock_mcp_server.py");
        let script_content = r#"import sys
import json

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue

    try:
        msg = json.loads(line)
    except Exception:
        continue

    # Ignore client responses (no method)
    if not isinstance(msg, dict) or "method" not in msg:
        continue

    method = msg.get("method")
    msg_id = msg.get("id")

    if method == "initialize":
        resp = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "serverInfo": {"name": "mock", "version": "1.0"}
            }
        }
        print(json.dumps(resp))
        sys.stdout.flush()
        continue

    if method == "tools/call":
        resp = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "content": [{"type": "text", "text": "tool ok"}]
            }
        }
        print(json.dumps(resp))
        sys.stdout.flush()
        continue
    
    if method == "notifications/roots/list_changed":
        # Simulate subprocess applying sandbox scopes and notify bridge
        print(json.dumps({"jsonrpc": "2.0", "method": "notifications/sandbox/configured"}))
        sys.stdout.flush()
        continue

    # Generic response for other request methods
    if msg_id is not None:
        print(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": {}}))
        sys.stdout.flush()
"#;

        fs::write(&script_path, script_content).expect("Failed to write mock MCP server script");
        script_path
    }

    #[tokio::test]
    async fn test_session_isolation_rejects_tool_calls_until_roots_lock_then_allows() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let script_path = write_mock_mcp_server_script(&temp_dir);

        let session_manager = Arc::new(SessionManager::new(SessionManagerConfig {
            server_command: "python3".to_string(),
            server_args: vec![script_path.to_string_lossy().to_string()],
            default_scope: Some(temp_dir.path().to_path_buf()),
            enable_colored_output: false,
            handshake_timeout_secs: DEFAULT_HANDSHAKE_TIMEOUT_SECS,
        }));

        let state = create_state_with_session_manager(Arc::clone(&session_manager));
        let app = create_app(state);

        let session_id = session_manager
            .create_session()
            .await
            .expect("Should create session");

        // 0) Send notifications/initialized to complete MCP handshake
        // (without this, the bridge will block waiting for initialization)
        let init_notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                    .body(Body::from(serde_json::to_vec(&init_notification).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify MCP is now initialized
        let session = session_manager
            .get_session(&session_id)
            .expect("Session should exist");
        assert!(session.is_mcp_initialized());

        // 1) tools/call should be rejected before sandbox is locked.
        let tool_call = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "dummy",
                "arguments": {}
            }
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                    .body(Body::from(serde_json::to_vec(&tool_call).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            response
                .headers()
                .get(MCP_SESSION_ID_HEADER)
                .and_then(|h| h.to_str().ok()),
            Some(session_id.as_str())
        );
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], -32001);

        // 2) Simulate client response to roots/list (this locks sandbox scope).
        let client_roots_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 999,
            "result": {
                "roots": [
                    {
                        "uri": format!("file://{}", temp_dir.path().display()),
                        "name": "root"
                    }
                ]
            }
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                    .body(Body::from(
                        serde_json::to_vec(&client_roots_response).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let session = session_manager
            .get_session(&session_id)
            .expect("Session should exist");
        assert!(session.is_sandbox_locked());

        let sandbox_scope = session
            .get_sandbox_scope()
            .await
            .expect("Sandbox scope should be set after roots lock");
        assert_eq!(sandbox_scope, temp_dir.path().to_path_buf());

        // 3) tools/call should now be forwarded and succeed.
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                    .body(Body::from(serde_json::to_vec(&tool_call).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        let text = json["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert_eq!(text, "tool ok");
    }

    #[tokio::test]
    async fn test_health_check_endpoint() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let script_path = write_mock_mcp_server_script(&temp_dir);
        let session_manager = Arc::new(SessionManager::new(SessionManagerConfig {
            server_command: "python3".to_string(),
            server_args: vec![script_path.to_string_lossy().to_string()],
            default_scope: Some(temp_dir.path().to_path_buf()),
            enable_colored_output: false,
            handshake_timeout_secs: DEFAULT_HANDSHAKE_TIMEOUT_SECS,
        }));
        let state = create_state_with_session_manager(session_manager);
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
}
