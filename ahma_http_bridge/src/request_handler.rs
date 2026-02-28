use crate::session::{McpRoot, SessionManager, request_timeout_secs, tool_call_timeout_secs};
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tracing::{debug, error, info, warn};

/// MCP Session-Id header name (per MCP spec 2025-03-26)
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Create a JSON response with appropriate headers
fn json_response(value: Value) -> Response {
    json_response_with_status(StatusCode::OK, value)
}

/// Create a JSON response with the provided status.
fn json_response_with_status(status: StatusCode, value: Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&value).unwrap_or_default()))
        .unwrap_or_else(|_| (status, "Failed to create response").into_response())
}

/// Build a JSON-RPC error object.
fn json_rpc_error_value(code: i32, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message
        }
    })
}

/// Attach MCP session header when available.
fn with_session_header(mut response: Response, session_id: &str) -> Response {
    let header_value = HeaderValue::from_str(session_id)
        .ok()
        .unwrap_or_else(|| HeaderValue::from_static("invalid"));
    response
        .headers_mut()
        .insert(MCP_SESSION_ID_HEADER, header_value);
    response
}

/// Create an error response with the provided status and JSON-RPC code.
fn error_response_with_status(status: StatusCode, code: i32, message: &str) -> Response {
    json_response_with_status(status, json_rpc_error_value(code, message))
}

/// Create an error response in the appropriate format
fn error_response(code: i32, message: &str) -> Response {
    error_response_with_status(StatusCode::INTERNAL_SERVER_ERROR, code, message)
}

/// Handles requests in session isolation mode.
pub async fn handle_session_isolated_request(
    session_manager: Arc<SessionManager>,
    headers: HeaderMap,
    payload: Value,
) -> Response {
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

    // 1. Handle "initialize" requests (session creation)
    if method == Some("initialize") && session_id.is_none() {
        return handle_initialize(&session_manager, &payload).await;
    }

    // 2. Route to existing session
    if let Some(session_id) = session_id {
        return handle_existing_session_request(&session_manager, &session_id, method, &payload)
            .await;
    }

    // 3. Handle incorrect requests (no session ID, not initialize)
    debug!(
        "Request without session ID for non-initialize method: {:?}",
        method
    );
    error_response_with_status(
        StatusCode::BAD_REQUEST,
        -32600,
        "Missing Mcp-Session-Id header. Send initialize request first.",
    )
}

/// Handles initialization requests by creating a new session.
async fn handle_initialize(session_manager: &SessionManager, payload: &Value) -> Response {
    debug!("Processing initialize request (no session ID)");

    // Fail fast on obviously invalid initialize requests.
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

    let new_session_id = match session_manager.create_session().await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to create session: {}", e);
            return error_response(-32603, &format!("Failed to create session: {}", e));
        }
    };

    info!(
        session_id = %new_session_id,
        "Session created, forwarding initialize request"
    );

    let init_timeout = Duration::from_secs(request_timeout_secs());
    match session_manager
        .send_request(&new_session_id, payload, Some(init_timeout))
        .await
    {
        Ok(response) => with_session_header(json_response(response), &new_session_id),
        Err(e) => {
            error!(
                session_id = %new_session_id,
                "Failed to send initialize request: {}", e
            );
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

/// Handles requests for an existing session.
async fn handle_existing_session_request(
    session_manager: &SessionManager,
    session_id: &str,
    method: Option<&str>,
    payload: &Value,
) -> Response {
    if !session_manager.session_exists(session_id) {
        warn!(
            session_id = %session_id,
            "Request for non-existent or terminated session"
        );
        return error_response_with_status(
            StatusCode::FORBIDDEN,
            -32600,
            "Session not found or terminated",
        );
    }

    // Check for roots/list_changed notification
    if method == Some("notifications/roots/list_changed")
        && let Err(e) = session_manager.handle_roots_changed(session_id).await
    {
        error!(session_id = %session_id, "Roots change rejected: {}", e);
        return error_response_with_status(
            StatusCode::FORBIDDEN,
            -32600,
            "Session terminated: roots change not allowed",
        );
    }

    // Delay tool execution if needed
    if method == Some("tools/call")
        && let Some(response) = check_sandbox_lock(session_manager, session_id)
    {
        return response;
    }

    let is_initialized_notification = method == Some("notifications/initialized");
    if is_initialized_notification {
        debug!(
            session_id = %session_id,
            "Received notifications/initialized"
        );
    }

    // Check if this is a CLIENT RESPONSE (has id + result/error, no method)
    let is_client_response = is_client_response(method, payload);

    // Wait for initialization if needed
    if let Some(response) = check_initialization_required(
        session_manager,
        session_id,
        method,
        is_initialized_notification,
        is_client_response,
    )
    .await
    {
        return response;
    }

    if is_client_response {
        return handle_client_response(session_manager, session_id, payload).await;
    }

    // Forward request to session
    forward_request(
        session_manager,
        session_id,
        method,
        payload,
        is_initialized_notification,
    )
    .await
}

/// Checks whether MCP initialization is required and waits for it if so.
///
/// Returns `Some(Response)` if initialization timed out, `None` to proceed.
async fn check_initialization_required(
    session_manager: &SessionManager,
    session_id: &str,
    method: Option<&str>,
    is_initialized_notification: bool,
    is_client_response: bool,
) -> Option<Response> {
    if is_initialized_notification || is_client_response {
        return None;
    }

    let session = session_manager.get_session(session_id)?;
    if session.is_mcp_initialized() {
        return None;
    }

    wait_for_initialization(&session, session_id, method).await
}

fn is_client_response(method: Option<&str>, payload: &Value) -> bool {
    method.is_none()
        && payload.get("id").is_some()
        && (payload.get("result").is_some() || payload.get("error").is_some())
}

/// Checks if the sandbox is locked for `tools/call` requests.
fn check_sandbox_lock(session_manager: &SessionManager, session_id: &str) -> Option<Response> {
    let session = session_manager.get_session(session_id)?;
    if session.is_sandbox_locked() {
        return None;
    }

    let sse_connected = session.is_sse_connected();
    let mcp_initialized = session.is_mcp_initialized();

    debug!(
        session_id = %session_id,
        sse_connected = sse_connected,
        mcp_initialized = mcp_initialized,
        sandbox_locked = false,
        "tools/call blocked - sandbox not yet locked"
    );

    if let Some(elapsed_secs) = session.is_handshake_timed_out() {
        let error_msg = handshake_timeout_message(
            elapsed_secs,
            sse_connected,
            mcp_initialized,
            session_manager.requires_client_roots(),
        );

        error!(
            session_id = %session_id,
            "Handshake timeout: SSE={}, initialized={}",
            sse_connected,
            mcp_initialized
        );

        return Some(with_session_header(
            error_response_with_status(StatusCode::GATEWAY_TIMEOUT, -32002, &error_msg),
            session_id,
        ));
    }

    let conflict_message = if session_manager.requires_client_roots() {
        "Sandbox initializing from client roots. This server requires roots/list from client; configure --sandbox-scope for clients without roots support."
    } else {
        "Sandbox initializing from client roots or explicit fallback scope - retry tools/call after handshake completes"
    };

    Some(with_session_header(
        error_response_with_status(StatusCode::CONFLICT, -32001, conflict_message),
        session_id,
    ))
}

/// Build the detailed error message for a handshake timeout.
fn handshake_timeout_message(
    elapsed_secs: u64,
    sse_connected: bool,
    mcp_initialized: bool,
    requires_roots: bool,
) -> String {
    let roots_requirement = if requires_roots {
        "No explicit server sandbox scope is configured; client roots/list is required."
    } else {
        "Server has explicit fallback sandbox scope configured for no-roots clients."
    };

    format!(
        "Handshake timeout after {}s - sandbox not locked. \
            SSE connected: {}, MCP initialized: {}. \
            Ensure client: 1) opens SSE stream (GET /mcp with session header), \
            2) sends notifications/initialized, \
            3) responds to roots/list request over SSE. {} \
            Use --handshake-timeout-secs to adjust timeout.",
        elapsed_secs, sse_connected, mcp_initialized, roots_requirement
    )
}

/// Waits for MCP initialization before forwarding a request.
async fn wait_for_initialization(
    session: &crate::session::Session,
    session_id: &str,
    method: Option<&str>,
) -> Option<Response> {
    debug!(
        session_id = %session_id,
        method = ?method,
        "Waiting for MCP initialization before forwarding request"
    );

    let init_timeout = Duration::from_secs(30);
    let wait_result = tokio::time::timeout(init_timeout, session.wait_for_mcp_initialized()).await;

    debug!(
        session_id = %session_id,
        method = ?method,
        "Wait for MCP initialization result: {:?}",
        wait_result
    );

    if wait_result.is_err() {
        warn!(
            session_id = %session_id,
            method = ?method,
            "Timeout waiting for MCP initialization"
        );
        return Some(with_session_header(
            error_response_with_status(
                StatusCode::GATEWAY_TIMEOUT,
                -32002,
                "Timeout waiting for MCP initialization - client must send notifications/initialized first",
            ),
            session_id,
        ));
    }
    debug!(
        session_id = %session_id,
        method = ?method,
        "MCP initialized, proceeding with request"
    );
    None
}

/// Handles a client response (e.g. to `roots/list`).
async fn handle_client_response(
    session_manager: &SessionManager,
    session_id: &str,
    payload: &Value,
) -> Response {
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
    if let Some(result) = payload.get("result") {
        try_lock_sandbox_from_roots(session_manager, session_id, result).await;
    }

    // Always forward response to subprocess
    if let Err(e) = session_manager.send_message(session_id, payload).await {
        error!(
            session_id = %session_id,
            "Failed to forward client response: {}", e
        );
        return error_response(-32603, &format!("Failed to forward response: {}", e));
    }

    with_session_header(
        json_response_with_status(StatusCode::ACCEPTED, serde_json::json!({})),
        session_id,
    )
}

/// Attempt to lock sandbox from a `roots/list` style result payload.
async fn try_lock_sandbox_from_roots(
    session_manager: &SessionManager,
    session_id: &str,
    result: &Value,
) {
    let Some(roots) = result.get("roots").and_then(|r| r.as_array()) else {
        return;
    };

    let mcp_roots: Vec<McpRoot> = roots
        .iter()
        .filter_map(|r| serde_json::from_value(r.clone()).ok())
        .collect();

    let should_lock = !mcp_roots.is_empty()
        || session_manager
            .get_session(session_id)
            .is_some_and(|s| s.is_sse_connected());

    if !should_lock {
        debug!(
            session_id = %session_id,
            "Skipping sandbox lock from empty roots/list response (SSE not connected yet)"
        );
        return;
    }

    info!(
        session_id = %session_id,
        roots = ?mcp_roots,
        "Locking sandbox from roots/list response"
    );

    match session_manager.lock_sandbox(session_id, &mcp_roots).await {
        Ok(true) => {
            info!(
                session_id = %session_id,
                "Sandbox locked from first roots/list response"
            );
        }
        Ok(false) => {}
        Err(e) => {
            warn!(
                session_id = %session_id,
                "Failed to record sandbox scopes: {}", e
            );
        }
    }
}

/// Forwards a request to the session manager.
async fn forward_request(
    session_manager: &SessionManager,
    session_id: &str,
    method: Option<&str>,
    payload: &Value,
    is_initialized_notification: bool,
) -> Response {
    let request_timeout = if method == Some("tools/call") {
        calculate_tool_timeout(payload)
    } else {
        Duration::from_secs(request_timeout_secs())
    };

    // NOTE: We previously waited here for subprocess sandbox-applied notification,
    // but this caused CI timeouts on slow Linux runners. The subprocess handles
    // race conditions by rejecting premature tool calls, and the test retry loop
    // handles retries. Removed per fix for test_roots_uri_parsing_percent_encoded_path.

    match session_manager
        .send_request(session_id, payload, Some(request_timeout))
        .await
    {
        Ok(response) => {
            if method == Some("roots/list")
                && let Some(result) = response.get("result")
            {
                try_lock_sandbox_from_roots(session_manager, session_id, result).await;
            }

            // Mark MCP as initialized BEFORE constructing response to prevent race conditions
            if is_initialized_notification
                && let Some(session) = session_manager.get_session(session_id)
                && let Err(e) = session.mark_mcp_initialized().await
            {
                warn!(
                    session_id = %session_id,
                    "Failed to mark MCP initialized: {}", e
                );
            }

            with_session_header(json_response(response), session_id)
        }
        Err(e) => {
            error!(session_id = %session_id, "Failed to send request: {}", e);
            error_response(-32603, &format!("Failed to send request: {}", e))
        }
    }
}

fn calculate_tool_timeout(payload: &Value) -> Duration {
    let arg_timeout_secs = payload
        .get("params")
        .and_then(|p| p.get("arguments"))
        .and_then(|a| a.get("timeout_seconds"))
        .and_then(|v| v.as_u64());

    let default_secs = tool_call_timeout_secs();
    let effective_secs = arg_timeout_secs
        .map(|v| v.min(600)) // Cap at 10 minutes
        .unwrap_or(default_secs);

    Duration::from_secs(effective_secs)
}
