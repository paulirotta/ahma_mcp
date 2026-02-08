use crate::session::{McpRoot, SessionManager, request_timeout_secs, tool_call_timeout_secs};
use axum::{
    body::Body,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tracing::{debug, error, info, warn};

/// MCP Session-Id header name (per MCP spec 2025-03-26)
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Create a JSON response with appropriate headers
fn json_response(value: Value) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&value).unwrap_or_default()))
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
        .body(Body::from(
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
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("content-type", "application/json")
        .body(Body::from(
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

    match session_manager.create_session().await {
        Ok(new_session_id) => {
            info!(
                session_id = %new_session_id,
                "Session created, forwarding initialize request"
            );

            let init_timeout = Duration::from_secs(request_timeout_secs());
            match session_manager
                .send_request(&new_session_id, payload, Some(init_timeout))
                .await
            {
                Ok(response) => {
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
        Err(e) => {
            error!("Failed to create session: {}", e);
            error_response(-32603, &format!("Failed to create session: {}", e))
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
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("content-type", "application/json")
            .body(Body::from(
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

    // Check for roots/list_changed notification
    if method == Some("notifications/roots/list_changed")
        && let Err(e) = session_manager.handle_roots_changed(session_id).await {
            error!(session_id = %session_id, "Roots change rejected: {}", e);
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "application/json")
                .body(Body::from(
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

    // Delay tool execution if needed
    if method == Some("tools/call")
        && let Some(response) = check_sandbox_lock(session_manager, session_id) {
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
    let is_client_response = method.is_none()
        && payload.get("id").is_some()
        && (payload.get("result").is_some() || payload.get("error").is_some());

    // Wait for initialization if needed
    if !is_initialized_notification && !is_client_response
        && let Some(session) = session_manager.get_session(session_id)
            && !session.is_mcp_initialized()
                && let Some(response) = wait_for_initialization(&session, session_id, method).await
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

/// Checks if the sandbox is locked for `tools/call` requests.
fn check_sandbox_lock(session_manager: &SessionManager, session_id: &str) -> Option<Response> {
    if let Some(session) = session_manager.get_session(session_id)
        && !session.is_sandbox_locked() {
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
                let error_msg = format!(
                    "Handshake timeout after {}s - sandbox not locked. \
                    SSE connected: {}, MCP initialized: {}. \
                    Ensure client: 1) opens SSE stream (GET /mcp with session header), \
                    2) sends notifications/initialized, \
                    3) responds to roots/list request over SSE. \
                    Use --handshake-timeout-secs to adjust timeout.",
                    elapsed_secs, sse_connected, mcp_initialized
                );

                error!(
                    session_id = %session_id,
                    "Handshake timeout: SSE={}, initialized={}",
                    sse_connected,
                    mcp_initialized
                );

                let error_json = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32002,
                        "message": error_msg
                    }
                });
                return Some(
                    Response::builder()
                        .status(StatusCode::GATEWAY_TIMEOUT)
                        .header("content-type", "application/json")
                        .header(MCP_SESSION_ID_HEADER, session_id)
                        .body(Body::from(
                            serde_json::to_vec(&error_json).unwrap_or_default(),
                        ))
                        .unwrap_or_else(|_| {
                            (StatusCode::GATEWAY_TIMEOUT, "Handshake timeout").into_response()
                        }),
                );
            }

            // Still initializing - return 409 Conflict
            let error_json = serde_json::json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32001,
                    "message": "Sandbox initializing from client roots - retry tools/call after roots/list completes"
                }
            });
            return Some(
                Response::builder()
                    .status(StatusCode::CONFLICT)
                    .header("content-type", "application/json")
                    .header(MCP_SESSION_ID_HEADER, session_id)
                    .body(Body::from(
                        serde_json::to_vec(&error_json).unwrap_or_default(),
                    ))
                    .unwrap_or_else(|_| {
                        (StatusCode::CONFLICT, "Sandbox initializing").into_response()
                    }),
            );
        }
    None
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
        let error_json = serde_json::json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32002,
                "message": "Timeout waiting for MCP initialization - client must send notifications/initialized first"
            }
        });
        return Some(
            Response::builder()
                .status(StatusCode::GATEWAY_TIMEOUT)
                .header("content-type", "application/json")
                .header(MCP_SESSION_ID_HEADER, session_id)
                .body(Body::from(
                    serde_json::to_vec(&error_json).unwrap_or_default(),
                ))
                .unwrap_or_else(|_| {
                    (StatusCode::GATEWAY_TIMEOUT, "Initialization timeout").into_response()
                }),
        );
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
    if let Some(result) = payload.get("result")
        && let Some(roots) = result.get("roots").and_then(|r| r.as_array()) {
            let mcp_roots: Vec<McpRoot> = roots
                .iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect();

            let should_lock = if mcp_roots.is_empty() {
                session_manager
                    .get_session(session_id)
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
        }

    // Always forward response to subprocess
    if let Err(e) = session_manager.send_message(session_id, payload).await {
        error!(
            session_id = %session_id,
            "Failed to forward client response: {}", e
        );
        return error_response(-32603, &format!("Failed to forward response: {}", e));
    }

    Response::builder()
        .status(StatusCode::ACCEPTED)
        .header("content-type", "application/json")
        .header(MCP_SESSION_ID_HEADER, session_id)
        .body(Body::from("{}"))
        .unwrap_or_else(|_| StatusCode::ACCEPTED.into_response())
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

    // If tools/call and sandbox is locked, wait for sandbox applied
    if method == Some("tools/call")
        && let Some(session) = session_manager.get_session(session_id)
            && session.is_sandbox_locked() && !session.is_sandbox_applied() {
                let wait_timeout = std::time::Duration::from_secs(15);
                let wait_result =
                    tokio::time::timeout(wait_timeout, session.wait_for_sandbox_applied()).await;
                if wait_result.is_err() {
                    warn!(
                        session_id = %session_id,
                        "Timed out waiting for subprocess sandbox-applied notification; forwarding tools/call optimistically"
                    );
                }
            }

    match session_manager
        .send_request(session_id, payload, Some(request_timeout))
        .await
    {
        Ok(response) => {
            // Check if roots/list response - lock sandbox
            if method == Some("roots/list")
                && let Some(result) = response.get("result")
                    && let Some(roots) = result.get("roots").and_then(|r| r.as_array()) {
                        let mcp_roots: Vec<McpRoot> = roots
                            .iter()
                            .filter_map(|r| serde_json::from_value(r.clone()).ok())
                            .collect();

                        let should_lock = if mcp_roots.is_empty() {
                            session_manager
                                .get_session(session_id)
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
                            session_manager.lock_sandbox(session_id, &mcp_roots).await
                        {
                            warn!(session_id = %session_id, "Failed to lock sandbox: {}", e);
                        }
                    }

            // Mark MCP as initialized BEFORE constructing response to prevent race conditions
            if is_initialized_notification
                && let Some(session) = session_manager.get_session(session_id)
                    && let Err(e) = session.mark_mcp_initialized().await {
                        warn!(
                            session_id = %session_id,
                            "Failed to mark MCP initialized: {}", e
                        );
                    }

            let mut http_response = json_response(response);
            http_response.headers_mut().insert(
                MCP_SESSION_ID_HEADER,
                session_id
                    .parse()
                    .unwrap_or_else(|_| "invalid".parse().unwrap()),
            );
            http_response
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
