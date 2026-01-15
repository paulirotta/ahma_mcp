//! HTTP-to-stdio bridge implementation

use crate::error::{BridgeError, Result};
use crate::session::{SessionManager, SessionManagerConfig};
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
use std::{convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
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
///     // Security: Always set a restrictive default scope!
///     default_sandbox_scope: PathBuf::from("/tmp/sandbox"),
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

    /// Fallback sandbox directory used if the client does not provide workspace roots.
    ///
    /// In session isolation mode, each session gets a unique subprocess. If the client
    /// handshake doesn't specify roots (e.g. empty `roots/list` response), this path
    /// limits the subprocess's file access.
    pub default_sandbox_scope: PathBuf,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            server_command: "ahma_mcp".to_string(),
            server_args: vec![],
            enable_colored_output: false,
            default_sandbox_scope: PathBuf::from("."),
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

    // Mark SSE as connected - if MCP is already initialized, this will trigger roots/list_changed
    if let Err(e) = session.mark_sse_connected().await {
        warn!(session_id = %session_id, "Failed to mark SSE connected: {}", e);
    }

    // Subscribe to the session's broadcast channel
    let rx = session.subscribe();

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

/// Create an error response in the appropriate format
fn error_response(code: i32, message: &str) -> Response {
    let error_json = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message
        }
    });

    Response::builder()
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
        })
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

    handle_session_isolated_request(state, headers, payload).await
}

/// Handle requests in session isolation mode
///
/// Per R8D:
/// - If no Mcp-Session-Id header and method is "initialize", create new session
/// - If Mcp-Session-Id header exists, route to that session
/// - Handle roots/list responses to lock sandbox scope
/// - Reject roots change after sandbox lock (terminate session)
async fn handle_session_isolated_request(
    state: Arc<BridgeState>,
    headers: HeaderMap,
    payload: Value,
) -> Response {
    use crate::session::McpRoot;

    let session_manager = &state.session_manager;

    // Get session ID from header
    let session_id = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let method = payload.get("method").and_then(|m| m.as_str());

    // Trace all incoming requests for debugging
    debug!(
        method = ?method,
        session_id = ?session_id,
        has_id = payload.get("id").is_some(),
        "Incoming MCP request"
    );

    // Handle session creation on initialize request (R8D.2)
    if method == Some("initialize") && session_id.is_none() {
        debug!("Processing initialize request (no session ID)");
        // Fail fast on obviously invalid initialize requests.
        // Without protocolVersion, the downstream stdio MCP server may never reply,
        // which turns into a confusing 60s bridge timeout.
        let protocol_version_ok = payload
            .get("params")
            .and_then(|p| p.get("protocolVersion"))
            .and_then(|v| v.as_str())
            .is_some();

        if !protocol_version_ok {
            return error_response(
                -32602,
                "Invalid initialize params: missing params.protocolVersion",
            );
        }

        info!("Creating new session for initialize request");

        match session_manager.create_session().await {
            Ok(new_session_id) => {
                info!(session_id = %new_session_id, "Session created, forwarding initialize request");

                // Forward the initialize request to the new session
                // Sandbox will be locked when client responds to roots/list via SSE
                match session_manager
                    .send_request(&new_session_id, &payload)
                    .await
                {
                    Ok(response) => {
                        // Return response with Mcp-Session-Id header (R8D.3)
                        let mut http_response = json_response(response);

                        http_response.headers_mut().insert(
                            MCP_SESSION_ID_HEADER,
                            new_session_id
                                .parse()
                                .unwrap_or_else(|_| "invalid".parse().unwrap()),
                        );

                        http_response
                    }
                    Err(e) => {
                        error!(session_id = %new_session_id, "Failed to send initialize request: {}", e);
                        // Clean up failed session
                        let _ = session_manager
                            .terminate_session(
                                &new_session_id,
                                crate::session::SessionTerminationReason::ProcessCrashed,
                            )
                            .await;
                        error_response(-32603, &format!("Failed to initialize session: {}", e))
                    }
                }
            }
            Err(e) => {
                error!("Failed to create session: {}", e);
                error_response(-32603, &format!("Failed to create session: {}", e))
            }
        }
    } else if let Some(session_id) = session_id {
        // Route to existing session (R8D.4)
        if !session_manager.session_exists(&session_id) {
            // Session not found or terminated (R8D.13)
            warn!(session_id = %session_id, "Request for non-existent or terminated session");
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {
                            "code": -32600,
                            "message": "Session not found or terminated"
                        }
                    }))
                    .unwrap_or_default(),
                ))
                .unwrap_or_else(|_| {
                    (StatusCode::FORBIDDEN, "Session not found or terminated").into_response()
                });
        }

        // Check for roots/list_changed notification (R8D.12)
        if method == Some("notifications/roots/list_changed")
            && let Err(e) = session_manager.handle_roots_changed(&session_id).await
        {
            error!(session_id = %session_id, "Roots change rejected: {}", e);
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "jsonrpc": "2.0",
                        "error": {
                            "code": -32600,
                            "message": "Session terminated: roots change not allowed"
                        }
                    }))
                    .unwrap_or_default(),
                ))
                .unwrap_or_else(|_| (StatusCode::FORBIDDEN, "Session terminated").into_response());
        }

        // Delay tool execution until sandbox is locked from first roots/list response.
        // This keeps the subprocess in a safe, non-executing state while sandbox initialization completes.
        // Also check for handshake timeout to provide actionable error messages.
        if method == Some("tools/call")
            && let Some(session) = session_manager.get_session(&session_id)
            && !session.is_sandbox_locked()
        {
            let sse_connected = session.is_sse_connected();
            let mcp_initialized = session.is_mcp_initialized();
            debug!(
                session_id = %session_id,
                sse_connected = sse_connected,
                mcp_initialized = mcp_initialized,
                sandbox_locked = false,
                "tools/call blocked - sandbox not yet locked"
            );

            // Check if handshake has timed out
            if let Some(elapsed_secs) = session.is_handshake_timed_out() {
                let sse_connected = session.is_sse_connected();
                let mcp_initialized = session.is_mcp_initialized();

                let error_msg = format!(
                    "Handshake timeout after {}s - sandbox not locked. \
                    SSE connected: {}, MCP initialized: {}. \
                    Ensure client: 1) opens SSE stream (GET /mcp with session header), \
                    2) sends notifications/initialized, \
                    3) responds to roots/list request over SSE. \
                    Set AHMA_HANDSHAKE_TIMEOUT_SECS to adjust timeout.",
                    elapsed_secs, sse_connected, mcp_initialized
                );

                error!(session_id = %session_id, "Handshake timeout: SSE={}, initialized={}", sse_connected, mcp_initialized);

                let error_json = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32002,
                        "message": error_msg
                    }
                });
                return Response::builder()
                    .status(StatusCode::GATEWAY_TIMEOUT)
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&error_json).unwrap_or_default(),
                    ))
                    .unwrap_or_else(|_| {
                        (StatusCode::GATEWAY_TIMEOUT, "Handshake timeout").into_response()
                    });
            }

            // Still initializing - return 409 Conflict
            let error_json = serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32001,
                    "message": "Sandbox initializing from client roots - retry tools/call after roots/list completes"
                }
            });
            return Response::builder()
                .status(StatusCode::CONFLICT)
                .header("content-type", "application/json")
                .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                .body(axum::body::Body::from(
                    serde_json::to_vec(&error_json).unwrap_or_default(),
                ))
                .unwrap_or_else(|_| {
                    (StatusCode::CONFLICT, "Sandbox initializing").into_response()
                });
        }

        let mut is_initialized_notification = false;

        // Check for initialized notification - this completes the MCP handshake
        // Once both SSE is connected AND initialized is received, we send roots/list_changed
        if method == Some("notifications/initialized") {
            debug!(session_id = %session_id, "Received notifications/initialized");
            is_initialized_notification = true;
        }

        // Check if this is a CLIENT RESPONSE (has id + result/error, no method)
        // This happens when server sends a request via SSE and client responds via POST
        let is_client_response = method.is_none()
            && payload.get("id").is_some()
            && (payload.get("result").is_some() || payload.get("error").is_some());

        // RACE CONDITION FIX: Wait for MCP initialization before forwarding requests.
        // Some clients (e.g., VS Code) may send requests like tools/list before notifications/initialized.
        // The MCP protocol (rmcp) requires the initialized notification to come first.
        // We gate non-init requests here with a timeout to ensure proper ordering.
        if !is_initialized_notification
            && !is_client_response
            && let Some(session) = session_manager.get_session(&session_id)
            && !session.is_mcp_initialized()
        {
            debug!(
                session_id = %session_id,
                method = ?method,
                "Waiting for MCP initialization before forwarding request"
            );
            // Wait for initialization with a reasonable timeout
            let init_timeout = Duration::from_secs(5);
            let wait_result =
                tokio::time::timeout(init_timeout, session.wait_for_mcp_initialized()).await;

            if wait_result.is_err() {
                warn!(
                    session_id = %session_id,
                    method = ?method,
                    "Timeout waiting for MCP initialization"
                );
                let error_json = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32002,
                        "message": "Timeout waiting for MCP initialization - client must send notifications/initialized first"
                    }
                });
                return Response::builder()
                    .status(StatusCode::GATEWAY_TIMEOUT)
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&error_json).unwrap_or_default(),
                    ))
                    .unwrap_or_else(|_| {
                        (StatusCode::GATEWAY_TIMEOUT, "Initialization timeout").into_response()
                    });
            }
            debug!(
                session_id = %session_id,
                method = ?method,
                "MCP initialized, proceeding with request"
            );
        }

        if is_client_response {
            let response_id = payload.get("id");
            let has_result = payload.get("result").is_some();
            let has_error = payload.get("error").is_some();
            debug!(
                session_id = %session_id,
                response_id = ?response_id,
                has_result = has_result,
                has_error = has_error,
                "Received client response (SSE callback), forwarding to subprocess"
            );

            // Check if this is a roots/list response - extract roots and lock sandbox
            if let Some(result) = payload.get("result")
                && let Some(roots) = result.get("roots").and_then(|r| r.as_array())
            {
                let mcp_roots: Vec<McpRoot> = roots
                    .iter()
                    .filter_map(|r| serde_json::from_value(r.clone()).ok())
                    .collect();

                // IMPORTANT: Do not lock the sandbox from an empty roots list *before* SSE is connected.
                // During early handshake, the bridge may return an empty roots response because the
                // client hasn't opened the SSE stream yet. Locking here would incorrectly bind the
                // session to default_sandbox_scope (often the server's own repo).
                let should_lock = if mcp_roots.is_empty() {
                    session_manager
                        .get_session(&session_id)
                        .is_some_and(|s| s.is_sse_connected())
                } else {
                    true
                };

                if !should_lock {
                    debug!(
                        session_id = %session_id,
                        "Skipping sandbox lock from empty roots/list response (SSE not connected yet)"
                    );
                } else {
                    info!(
                        session_id = %session_id,
                        roots = ?mcp_roots,
                        "Locking sandbox from roots/list response"
                    );

                    match session_manager.lock_sandbox(&session_id, &mcp_roots).await {
                        Ok(true) => {
                            info!(session_id = %session_id, "Sandbox locked from first roots/list response");
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!(session_id = %session_id, "Failed to record sandbox scopes: {}", e)
                        }
                    }
                }
            }

            // Always forward the response to the subprocess so rmcp can resolve the pending request.
            if let Err(e) = session_manager.send_message(&session_id, &payload).await {
                error!(session_id = %session_id, "Failed to forward client response: {}", e);
                return error_response(-32603, &format!("Failed to forward response: {}", e));
            }

            // Return 202 Accepted - no response expected
            return Response::builder()
                .status(StatusCode::ACCEPTED)
                .header("content-type", "application/json")
                .header(MCP_SESSION_ID_HEADER, session_id.as_str())
                .body(axum::body::Body::from("{}"))
                .unwrap_or_else(|_| StatusCode::ACCEPTED.into_response());
        }

        // Forward request to session
        match session_manager.send_request(&session_id, &payload).await {
            Ok(response) => {
                // Check if this is a roots/list response - lock sandbox (R8D.7-R8D.8)
                if method == Some("roots/list")
                    && let Some(result) = response.get("result")
                    && let Some(roots) = result.get("roots").and_then(|r| r.as_array())
                {
                    let mcp_roots: Vec<McpRoot> = roots
                        .iter()
                        .filter_map(|r| serde_json::from_value(r.clone()).ok())
                        .collect();

                    let should_lock = if mcp_roots.is_empty() {
                        session_manager
                            .get_session(&session_id)
                            .is_some_and(|s| s.is_sse_connected())
                    } else {
                        true
                    };

                    if !should_lock {
                        debug!(
                            session_id = %session_id,
                            "Skipping sandbox lock from empty roots/list response (SSE not connected yet)"
                        );
                    } else if let Err(e) =
                        session_manager.lock_sandbox(&session_id, &mcp_roots).await
                    {
                        warn!(session_id = %session_id, "Failed to lock sandbox: {}", e);
                        // Don't fail the request, just log the warning
                    }
                }

                // Add session ID to response header
                let mut http_response = json_response(response);

                http_response.headers_mut().insert(
                    MCP_SESSION_ID_HEADER,
                    session_id
                        .parse()
                        .unwrap_or_else(|_| "invalid".parse().unwrap()),
                );

                if is_initialized_notification
                    && let Some(session) = session_manager.get_session(&session_id)
                    && let Err(e) = session.mark_mcp_initialized().await
                {
                    warn!(session_id = %session_id, "Failed to mark MCP initialized: {}", e);
                }

                http_response
            }
            Err(e) => {
                error!(session_id = %session_id, "Failed to send request: {}", e);
                error_response(-32603, &format!("Failed to send request: {}", e))
            }
        }
    } else {
        // No session ID and not an initialize request (R8D.5)
        // This is a client error, not a server issue - use debug level
        debug!(
            "Request without session ID for non-initialize method: {:?}",
            method
        );
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32600,
                        "message": "Missing Mcp-Session-Id header. Send initialize request first."
                    }
                }))
                .unwrap_or_default(),
            ))
            .unwrap_or_else(|_| (StatusCode::BAD_REQUEST, "Missing session ID").into_response())
    }
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
            default_sandbox_scope: PathBuf::from("/tmp"),
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
            default_scope: temp_dir.path().to_path_buf(),
            enable_colored_output: false,
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
            default_scope: temp_dir.path().to_path_buf(),
            enable_colored_output: false,
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
