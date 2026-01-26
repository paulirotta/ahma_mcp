//! Session management for HTTP bridge session isolation mode.
//!
//! Per R8D, session isolation allows multiple IDE instances to share a single
//! HTTP server with per-session sandbox scopes. Each session spawns a separate
//! `ahma_mcp` subprocess with its own sandbox scope derived from the client's
//! workspace roots.
//!
//! ## Overview
//!
//! In HTTP mode, the server spawns a separate `ahma_mcp` subprocess per MCP session.
//! Each subprocess has its own sandbox scope derived from the client's workspace roots,
//! providing complete isolation between concurrent sessions.
//!
//! ## How It Works
//!
//! ### Protocol Flow
//!
//! 1. **Receive initialize request**: Generate session ID (UUID).
//! 2. **Spawn ahma_mcp subprocess**: Start the MCP engine for this session.
//! 3. **Forward initialize**: Hand over the initialization to the subprocess.
//! 4. **Subprocess requests roots/list**: The engine asks for workspace context.
//! 5. **Bridge intercept**: Capture the roots to define the sandbox boundary.
//!
//! ### Sandbox Scope Binding
//!
//! The sandbox scope is determined lazily via the MCP `roots/list` protocol:
//! 1. Client sends `initialize` with `capabilities.roots: { listChanged: true }`.
//! 2. Server spawns subprocess without sandbox restriction initially.
//! 3. Subprocess sends `roots/list` request to get workspace folders.
//! 4. Bridge intercepts and caches the first root as sandbox scope.
//! 5. Subsequent file operations are validated against this scope.
//!
//! **Security Invariant**: The sandbox scope is set **once** when the first `roots/list`
//! response is received and cannot be changed for that session.
//!
//! ### Handling Roots Changes
//!
//! For security, session isolation mode rejects roots changes after the sandbox is locked.
//! If `notifications/roots/list_changed` is received after locking, the subprocess is
//! immediately terminated to prevent sandbox escape.

use crate::error::{BridgeError, Result};
use chrono::Local;
use dashmap::DashMap;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, Notify, broadcast, mpsc, oneshot},
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Handshake state machine for MCP session coordination.
///
/// The MCP Streamable HTTP protocol requires a specific sequence:
/// 1. Client sends `initialize` request → server creates session
/// 2. Client opens SSE stream (GET /mcp with session header)
/// 3. Client sends `notifications/initialized` notification
/// 4. Server sends `notifications/roots/list_changed` to subprocess
/// 5. Subprocess sends `roots/list` request back through SSE
/// 6. Client responds with roots → sandbox is locked
///
/// This enum tracks the state to ensure `roots/list_changed` is sent
/// exactly once, only when both SSE and MCP initialized are complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HandshakeState {
    /// Initial state: session created, waiting for SSE and MCP initialized
    AwaitingBoth = 0,
    /// SSE connected, waiting for MCP initialized notification
    AwaitingSseOnly = 1,
    /// MCP initialized received, waiting for SSE connection
    AwaitingMcpOnly = 2,
    /// Both SSE and MCP initialized, roots/list_changed sent, awaiting sandbox lock
    RootsRequested = 3,
    /// Sandbox locked, handshake complete
    Complete = 4,
}

impl HandshakeState {
    /// Convert from u8 to HandshakeState. Invalid values fall back to AwaitingBoth.
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::AwaitingBoth,
            1 => Self::AwaitingSseOnly,
            2 => Self::AwaitingMcpOnly,
            3 => Self::RootsRequested,
            4 => Self::Complete,
            _ => Self::AwaitingBoth, // Fallback to initial state
        }
    }
}

/// Represents a workspace root provided by the client's `roots/list` capability.
///
/// Used to determine the valid file access scope for the session's sandbox.
///
/// # Example JSON
///
/// ```json
/// {
///   "uri": "file:///Users/jdoe/project",
///   "name": "My Project"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRoot {
    /// URI of the root (must be file://)
    pub uri: String,
    /// Optional human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Get the handshake timeout in seconds from AHMA_HANDSHAKE_TIMEOUT_SECS env var.
/// Defaults to 30 seconds if not set or invalid.
pub fn handshake_timeout_secs() -> u64 {
    std::env::var("AHMA_HANDSHAKE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
}

/// Get the request timeout in seconds for bridge → subprocess calls.
/// Defaults to 60 seconds if not set or invalid.
pub fn request_timeout_secs() -> u64 {
    std::env::var("AHMA_HTTP_BRIDGE_REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
}

/// Get the tools/call request timeout in seconds.
/// Defaults to 25 seconds if not set or invalid.
pub fn tool_call_timeout_secs() -> u64 {
    std::env::var("AHMA_HTTP_BRIDGE_TOOL_CALL_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(25)
}

/// Session termination reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTerminationReason {
    /// Client requested termination (HTTP DELETE)
    ClientRequested,
    /// Roots change attempted after sandbox lock (security violation)
    RootsChangeRejected,
    /// Subprocess crashed
    ProcessCrashed,
    /// Session timed out
    Timeout,
}

/// Represents an active client session.
///
/// This struct holds the state for a single connection, including:
/// - The subprocess handle.
/// - The communication channels (SSE broadcast, request/response tracking).
/// - The security context (sandbox scopes).
///
/// Instances are created by `SessionManager` and accessed internally by the bridge.
///
/// # Security
///
/// The session ensures strict isolation by:
/// 1. Managing a dedicated subprocess
/// 2. Enforcing sandbox locking once roots are determined
/// 3. Validating handshake state transitions
pub struct Session {
    /// Unique session identifier
    pub id: String,
    /// Channel to send messages to the subprocess (wrapped in Mutex for restart support)
    sender: Mutex<mpsc::Sender<String>>,
    /// Map of pending request IDs to response channels
    pending_requests: Arc<DashMap<String, oneshot::Sender<Value>>>,
    /// Broadcast channel for SSE events from this session
    broadcast_tx: broadcast::Sender<String>,
    /// Sandbox scopes (set on first roots/list response) - supports multiple roots
    sandbox_scopes: Mutex<Option<Vec<PathBuf>>>,
    /// Whether sandbox has been locked (cannot change after first set)
    sandbox_locked: AtomicBool,
    /// Whether the session has been terminated
    terminated: AtomicBool,
    /// Termination reason (if terminated)
    termination_reason: Mutex<Option<SessionTerminationReason>>,
    /// Handle to the subprocess (for cleanup)
    child_handle: Mutex<Option<Child>>,
    /// Handshake state machine for SSE/MCP coordination (replaces separate AtomicBools)
    handshake_state: AtomicU8,
    /// Notify for waiting on MCP initialization (signals when handshake advances past AwaitingMcpOnly)
    mcp_initialized_notify: Notify,
    /// When the session was created (for handshake timeout tracking)
    created_at: Instant,
    /// Per-session handshake timeout duration captured at session creation to avoid env races
    handshake_timeout: Duration,
}

impl Session {
    /// Check if the session is terminated
    pub fn is_terminated(&self) -> bool {
        self.terminated.load(Ordering::SeqCst)
    }

    /// Check if the sandbox is locked
    ///
    /// The sandbox becomes locked after the first `roots/list` response is received
    /// from the client. Before this point, tools are blocked from executing.
    pub fn is_sandbox_locked(&self) -> bool {
        self.sandbox_locked.load(Ordering::SeqCst)
    }

    /// Get the current handshake state
    pub fn handshake_state(&self) -> HandshakeState {
        HandshakeState::from_u8(self.handshake_state.load(Ordering::SeqCst))
    }

    /// Check if the SSE stream is connected (client opened GET /mcp)
    pub fn is_sse_connected(&self) -> bool {
        matches!(
            self.handshake_state(),
            HandshakeState::AwaitingSseOnly
                | HandshakeState::RootsRequested
                | HandshakeState::Complete
        )
    }

    /// Check if MCP initialized notification was received
    pub fn is_mcp_initialized(&self) -> bool {
        matches!(
            self.handshake_state(),
            HandshakeState::AwaitingMcpOnly
                | HandshakeState::RootsRequested
                | HandshakeState::Complete
        )
    }

    /// Wait for MCP initialization.
    /// Returns immediately if already initialized, otherwise waits for notification.
    pub async fn wait_for_mcp_initialized(&self) {
        if self.is_mcp_initialized() {
            return;
        }
        self.mcp_initialized_notify.notified().await;
    }

    /// Check if the handshake has timed out (sandbox not locked within timeout).
    /// Returns Some(elapsed_secs) if timed out, None otherwise.
    pub fn is_handshake_timed_out(&self) -> Option<u64> {
        if self.is_sandbox_locked() {
            return None;
        }
        let elapsed = self.created_at.elapsed();
        if elapsed >= self.handshake_timeout {
            Some(elapsed.as_secs())
        } else {
            None
        }
    }

    /// Get the first sandbox scope (for backwards compatibility)
    pub async fn get_sandbox_scope(&self) -> Option<PathBuf> {
        self.sandbox_scopes
            .lock()
            .await
            .as_ref()
            .and_then(|v| v.first().cloned())
    }

    /// Get all sandbox scopes
    pub async fn get_sandbox_scopes(&self) -> Option<Vec<PathBuf>> {
        self.sandbox_scopes.lock().await.clone()
    }

    /// Subscribe to SSE events from this session
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcast_tx.subscribe()
    }

    /// Mark SSE as connected and trigger roots/list_changed if MCP is already initialized.
    ///
    /// State machine transitions:
    /// - AwaitingBoth → AwaitingSseOnly (SSE connected first, wait for MCP)
    /// - AwaitingMcpOnly → RootsRequested (MCP was already initialized, send roots/list_changed)
    ///
    /// Returns true if roots/list_changed was sent.
    pub async fn mark_sse_connected(&self) -> Result<bool> {
        let elapsed_ms = self.created_at.elapsed().as_millis();

        // Atomically transition state and determine if we should send roots/list_changed
        let should_send = loop {
            let current = self.handshake_state.load(Ordering::SeqCst);
            let current_state = HandshakeState::from_u8(current);

            let (new_state, send_notification) = match current_state {
                HandshakeState::AwaitingBoth => (HandshakeState::AwaitingSseOnly, false),
                HandshakeState::AwaitingMcpOnly => (HandshakeState::RootsRequested, true),
                // Already in a later state, no-op
                _ => {
                    info!(
                        session_id = %self.id,
                        elapsed_ms = elapsed_ms,
                        state = ?current_state,
                        "SSE connect called but state already advanced"
                    );
                    return Ok(false);
                }
            };

            match self.handshake_state.compare_exchange(
                current,
                new_state as u8,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    info!(
                        session_id = %self.id,
                        elapsed_ms = elapsed_ms,
                        from = ?current_state,
                        to = ?new_state,
                        "SSE connected, state transition"
                    );
                    break send_notification;
                }
                Err(_) => continue, // State changed, retry
            }
        };

        if should_send {
            self.send_roots_list_changed().await?;
        }
        Ok(should_send)
    }

    /// Mark MCP as initialized (handshake complete) and trigger roots/list_changed if SSE is already connected.
    ///
    /// State machine transitions:
    /// - AwaitingBoth → AwaitingMcpOnly (MCP initialized first, wait for SSE)
    /// - AwaitingSseOnly → RootsRequested (SSE was already connected, send roots/list_changed)
    ///
    /// Returns true if roots/list_changed was sent.
    pub async fn mark_mcp_initialized(&self) -> Result<bool> {
        let elapsed_ms = self.created_at.elapsed().as_millis();

        // Atomically transition state and determine if we should send roots/list_changed
        let should_send = loop {
            let current = self.handshake_state.load(Ordering::SeqCst);
            let current_state = HandshakeState::from_u8(current);

            let (new_state, send_notification) = match current_state {
                HandshakeState::AwaitingBoth => (HandshakeState::AwaitingMcpOnly, false),
                HandshakeState::AwaitingSseOnly => (HandshakeState::RootsRequested, true),
                // Already in a later state, no-op
                _ => {
                    info!(
                        session_id = %self.id,
                        elapsed_ms = elapsed_ms,
                        state = ?current_state,
                        "MCP initialized called but state already advanced"
                    );
                    // Still notify waiters in case someone is waiting
                    self.mcp_initialized_notify.notify_waiters();
                    return Ok(false);
                }
            };

            match self.handshake_state.compare_exchange(
                current,
                new_state as u8,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    // Wake up any waiters blocked on initialization
                    self.mcp_initialized_notify.notify_waiters();
                    info!(
                        session_id = %self.id,
                        elapsed_ms = elapsed_ms,
                        from = ?current_state,
                        to = ?new_state,
                        "MCP initialized, state transition"
                    );
                    break send_notification;
                }
                Err(_) => continue, // State changed, retry
            }
        };

        if should_send {
            self.send_roots_list_changed().await?;
        }
        Ok(should_send)
    }

    /// Mark handshake as complete (sandbox locked).
    /// This is called after roots/list response is processed.
    pub fn mark_handshake_complete(&self) {
        let current = self.handshake_state.load(Ordering::SeqCst);
        if current == HandshakeState::RootsRequested as u8 {
            self.handshake_state
                .store(HandshakeState::Complete as u8, Ordering::SeqCst);
            info!(session_id = %self.id, "Handshake complete");
        }
    }

    /// Send roots/list_changed notification to subprocess.
    /// This triggers the server to call roots/list, which goes back through SSE.
    async fn send_roots_list_changed(&self) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        });
        let json_str = serde_json::to_string(&notification)?;
        self.sender.lock().await.send(json_str).await.map_err(|e| {
            BridgeError::Communication(format!("Failed to send roots/list_changed: {}", e))
        })?;
        info!(session_id = %self.id, "Sent roots/list_changed notification to subprocess");
        Ok(())
    }
}

/// Configuration for the `SessionManager`.
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    /// The executable command to start the MCP server (e.g., "ahma_mcp").
    pub server_command: String,
    /// Arguments to pass to the server executable.
    pub server_args: Vec<String>,
    /// The fallback directory to use for the sandbox if the client doesn't provide roots.
    pub default_scope: PathBuf,
    /// Whether to preserve ANSI colors in the server's output streams.
    pub enable_colored_output: bool,
}

/// Manages the lifecycle of concurrent MCP sessions.
///
/// This component is responsible for:
/// - creating new sessions with unique IDs.
/// - spawning isolated subprocesses for each session.
/// - tracking active sessions in a thread-safe map.
/// - handling session termination.
///
/// Each session operates in its own `ahma_mcp` subprocess, ensuring that file access
/// rights and state are strictly isolated between different clients.
///
/// Use `create_session` to start a new handshake, and `lock_sandbox` to finalize security.
pub struct SessionManager {
    /// Active sessions indexed by session ID
    sessions: DashMap<String, Arc<Session>>,
    /// Configuration for spawning new sessions
    config: SessionManagerConfig,
}

impl SessionManager {
    fn parse_file_uri_to_path(uri: &str) -> Option<PathBuf> {
        // RFC 8089-ish minimal parsing: accept file:///abs/path and file://localhost/abs/path.
        // Percent-decoding is required for common IDE roots that contain spaces/unicode.
        const PREFIX: &str = "file://";
        if !uri.starts_with(PREFIX) {
            return None;
        }

        // Remove scheme.
        let mut rest = &uri[PREFIX.len()..];

        // Strip any query/fragment.
        if let Some(idx) = rest.find(['?', '#']) {
            rest = &rest[..idx];
        }

        // Handle host form: file://localhost/...
        if let Some(after_localhost) = rest.strip_prefix("localhost") {
            rest = after_localhost;
        }

        // For unix-like paths, we only accept absolute paths.
        if !rest.starts_with('/') {
            return None;
        }

        let decoded = Self::percent_decode_utf8(rest)?;
        Some(PathBuf::from(decoded))
    }

    fn percent_decode_utf8(input: &str) -> Option<String> {
        // Decode %XX sequences into bytes, then UTF-8.
        // If decoding fails, return None so we fall back to default scope.
        let bytes = input.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'%' => {
                    if i + 2 >= bytes.len() {
                        return None;
                    }
                    let hi = bytes[i + 1];
                    let lo = bytes[i + 2];
                    let hex = |b: u8| -> Option<u8> {
                        match b {
                            b'0'..=b'9' => Some(b - b'0'),
                            b'a'..=b'f' => Some(b - b'a' + 10),
                            b'A'..=b'F' => Some(b - b'A' + 10),
                            _ => None,
                        }
                    };
                    let hi = hex(hi)?;
                    let lo = hex(lo)?;
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8(out).ok()
    }

    /// Creates a new `SessionManager` with the given configuration.
    pub fn new(config: SessionManagerConfig) -> Self {
        Self {
            sessions: DashMap::new(),
            config,
        }
    }

    /// Initializes a new session and spawns a specific `ahma_mcp` subprocess for it.
    ///
    /// This initiates the "deferred sandbox" flow:
    /// 1. A new session ID is generated.
    /// 2. The subprocess is started with `--defer-sandbox`.
    /// 3. The subprocess waits for the bridge to provide the sandbox scope (derived from client roots).
    ///
    /// # Returns
    ///
    /// * `Ok(String)`: The new session ID (UUID v4). This ID must be included in the `Mcp-Session-Id` header for all subsequent requests.
    /// * `Err(BridgeError)`: If the subprocess could not be spawned.
    pub async fn create_session(&self) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();

        info!(session_id = %session_id, "Creating new session");

        // Spawn subprocess WITHOUT sandbox restriction initially
        // Sandbox will be applied when roots/list is received
        let stderr_mode = if self.config.enable_colored_output {
            Stdio::piped()
        } else {
            Stdio::inherit()
        };

        // Add --defer-sandbox so the subprocess waits for roots/list to set sandbox scope
        let mut args = self.config.server_args.clone();
        args.push("--defer-sandbox".to_string());

        let mut child = Command::new(&self.config.server_command)
            .args(&args)
            // SECURITY:
            // Avoid inheriting env vars that can auto-enable permissive test mode in ahma_core,
            // which can mask real sandbox-scoping behavior.
            .env_remove("AHMA_TEST_MODE")
            .env_remove("NEXTEST")
            .env_remove("NEXTEST_EXECUTION_MODE")
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("RUST_TEST_THREADS")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr_mode)
            // Ensure the subprocess does not outlive this process in the event a test exits early
            // or fails to explicitly terminate the session.
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                BridgeError::ServerProcess(format!("Failed to spawn subprocess: {}", e))
            })?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stderr = if self.config.enable_colored_output {
            child.stderr.take()
        } else {
            None
        };

        // Create channels
        let (tx, rx) = mpsc::channel::<String>(100);
        let (broadcast_tx, _) = broadcast::channel::<String>(100);
        let pending_requests = Arc::new(DashMap::new());

        let handshake_timeout = Duration::from_secs(handshake_timeout_secs());

        let session = Arc::new(Session {
            id: session_id.clone(),
            sender: Mutex::new(tx),
            pending_requests: pending_requests.clone(),
            broadcast_tx: broadcast_tx.clone(),
            sandbox_scopes: Mutex::new(None),
            sandbox_locked: AtomicBool::new(false),
            terminated: AtomicBool::new(false),
            termination_reason: Mutex::new(None),
            child_handle: Mutex::new(Some(child)),
            handshake_state: AtomicU8::new(HandshakeState::AwaitingBoth as u8),
            mcp_initialized_notify: Notify::new(),
            created_at: Instant::now(),
            handshake_timeout,
        });

        // Spawn the I/O handler task
        let session_clone = session.clone();
        let colored_output = self.config.enable_colored_output;
        tokio::spawn(async move {
            Self::handle_session_io(session_clone, rx, stdin, stdout, stderr, colored_output).await;
        });

        self.sessions.insert(session_id.clone(), session);

        Ok(session_id)
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<Arc<Session>> {
        self.sessions.get(session_id).map(|s| s.clone())
    }

    /// Send a message to a session's subprocess
    pub async fn send_message(&self, session_id: &str, message: &Value) -> Result<()> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        if session.is_terminated() {
            return Err(BridgeError::Communication(
                "Session has been terminated".to_string(),
            ));
        }

        let json_str = serde_json::to_string(message)?;
        session
            .sender
            .lock()
            .await
            .send(json_str)
            .await
            .map_err(|e| {
                BridgeError::Communication(format!("Failed to send to subprocess: {}", e))
            })?;

        Ok(())
    }

    /// Send a request and wait for response
    pub async fn send_request(
        &self,
        session_id: &str,
        request: &Value,
        timeout: Option<Duration>,
    ) -> Result<Value> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        if session.is_terminated() {
            return Err(BridgeError::Communication(
                "Session has been terminated".to_string(),
            ));
        }

        // Get request ID - treat null as absent (notification)
        let id_opt = request.get("id").and_then(|id| {
            if id.is_null() {
                None
            } else if id.is_string() {
                Some(id.as_str().unwrap().to_string())
            } else {
                Some(id.to_string())
            }
        });

        let (response_tx, response_rx) = if id_opt.is_some() {
            let (tx, rx) = oneshot::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // Register pending request
        if let Some(id) = &id_opt {
            session
                .pending_requests
                .insert(id.clone(), response_tx.unwrap());
        }

        // Send the request
        let json_str = serde_json::to_string(request)?;
        session
            .sender
            .lock()
            .await
            .send(json_str)
            .await
            .map_err(|e| {
                if let Some(id) = &id_opt {
                    session.pending_requests.remove(id);
                }
                BridgeError::Communication(format!("Failed to send to subprocess: {}", e))
            })?;

        // Wait for response if this is a request (has ID)
        if let Some(rx) = response_rx {
            let wait_timeout = timeout.unwrap_or_else(|| Duration::from_secs(request_timeout_secs()));
            match tokio::time::timeout(wait_timeout, rx).await {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(_)) => Err(BridgeError::Communication(
                    "Response channel closed".to_string(),
                )),
                Err(_) => {
                    if let Some(id) = &id_opt {
                        session.pending_requests.remove(id);
                    }
                    Err(BridgeError::Communication("Request timed out".to_string()))
                }
            }
        } else {
            // Notification, return success immediately
            Ok(serde_json::json!({"jsonrpc": "2.0", "result": null}))
        }
    }

    /// Lock sandbox scope for a session (called when observing first roots/list response).
    ///
    /// Per R8.4.4-R8.4.5, sandbox scope is determined from the first roots/list response
    /// and cannot be changed. In the simplified design, the subprocess is spawned with
    /// `--defer-sandbox` and configures its own sandbox after roots are received.
    ///
    /// This method only records the scopes for bridge-side enforcement (e.g. rejecting
    /// roots changes after lock) and for debugging.
    ///
    /// Returns `true` if the sandbox was newly locked by this call.
    /// Returns an error if no valid roots are provided (empty roots or all malformed URIs).
    pub async fn lock_sandbox(&self, session_id: &str, roots: &[McpRoot]) -> Result<bool> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        if session.sandbox_locked.load(Ordering::SeqCst) {
            return Ok(false);
        }

        // Extract all roots as sandbox scopes
        let scopes: Vec<PathBuf> = roots
            .iter()
            .filter_map(|r| Self::parse_file_uri_to_path(&r.uri))
            .collect();

        // SECURITY: Reject empty roots or roots where all URIs are malformed.
        // This prevents accidental over-permissive behavior by ensuring the sandbox
        // is bound to an explicit client-provided scope, not a server default.
        if scopes.is_empty() {
            warn!(
                session_id = %session_id,
                provided_roots = roots.len(),
                "Rejecting sandbox lock: no valid file:// URIs in roots/list response"
            );
            return Err(BridgeError::Communication(
                "No valid sandbox roots provided. Client must provide at least one valid file:// URI in roots/list response.".to_string()
            ));
        }

        info!(
            session_id = %session_id,
            sandbox_scopes = ?scopes,
            "Locking sandbox scope(s) for session"
        );

        // Store the sandbox scopes
        *session.sandbox_scopes.lock().await = Some(scopes.clone());
        session.sandbox_locked.store(true, Ordering::SeqCst);

        // Transition handshake state to Complete
        session.mark_handshake_complete();

        Ok(true)
    }

    /// Handle roots/list_changed notification
    ///
    /// Per R8D.12-R8D.13, if sandbox is locked, terminate the session immediately
    pub async fn handle_roots_changed(&self, session_id: &str) -> Result<()> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        if session.sandbox_locked.load(Ordering::SeqCst) {
            // Security violation: attempt to change roots after sandbox lock
            let scopes = session.sandbox_scopes.lock().await.clone();
            error!(
                session_id = %session_id,
                sandbox_scopes = ?scopes,
                "Roots change rejected after sandbox lock - terminating session"
            );

            // Drop the session reference before terminating to avoid deadlock
            drop(session);

            self.terminate_session(session_id, SessionTerminationReason::RootsChangeRejected)
                .await?;

            return Err(BridgeError::Communication(
                "Session terminated: roots change not allowed after sandbox lock".to_string(),
            ));
        }

        // Sandbox not yet locked - this is unusual but allowed
        // (roots/list hasn't been processed yet)
        warn!(
            session_id = %session_id,
            "Roots change received before sandbox lock - allowing"
        );
        Ok(())
    }

    /// Terminate a session
    pub async fn terminate_session(
        &self,
        session_id: &str,
        reason: SessionTerminationReason,
    ) -> Result<()> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            info!(
                session_id = %session_id,
                reason = ?reason,
                "Terminating session"
            );

            session.terminated.store(true, Ordering::SeqCst);
            *session.termination_reason.lock().await = Some(reason);

            // Kill the subprocess
            if let Some(mut child) = session.child_handle.lock().await.take() {
                let _ = child.kill().await;
            }

            // Clear pending requests
            session.pending_requests.clear();
        }

        Ok(())
    }

    /// Check if a session exists and is not terminated
    pub fn session_exists(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|s| !s.is_terminated())
            .unwrap_or(false)
    }

    /// Get session count (for metrics/debugging)
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Handle I/O for a session subprocess
    async fn handle_session_io(
        session: Arc<Session>,
        mut rx: mpsc::Receiver<String>,
        mut stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
        stderr: Option<tokio::process::ChildStderr>,
        colored_output: bool,
    ) {
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = stderr.map(|s| BufReader::new(s).lines());

        loop {
            tokio::select! {
                // Handle outgoing messages (HTTP -> Stdio)
                Some(msg) = rx.recv() => {
                    debug!(session_id = %session.id, "Sending to subprocess: {}", msg);

                    // Echo STDIN in cyan if colored output is enabled
                    if colored_output {
                        let timestamp = format!("[{}]", Local::now().format("%H:%M:%S%.3f"));
                        if let Ok(parsed) = serde_json::from_str::<Value>(&msg) {
                            let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| msg.clone());
                            eprintln!("{} {} {}\n{}", timestamp, format!("[{}]", &session.id[..8]).cyan(), "→ STDIN:".cyan(), pretty.cyan());
                        } else {
                            eprintln!("{} {} {}\n{}", timestamp, format!("[{}]", &session.id[..8]).cyan(), "→ STDIN:".cyan(), msg.cyan());
                        }
                    }

                    if let Err(e) = stdin.write_all(msg.as_bytes()).await {
                        error!(session_id = %session.id, "Failed to write to stdin: {}", e);
                        break;
                    }
                    if let Err(e) = stdin.write_all(b"\n").await {
                        error!(session_id = %session.id, "Failed to write newline to stdin: {}", e);
                        break;
                    }
                    if let Err(e) = stdin.flush().await {
                        error!(session_id = %session.id, "Failed to flush stdin: {}", e);
                        break;
                    }
                }

                // Handle incoming messages (Stdio -> HTTP/SSE)
                stdout_result = stdout_reader.next_line() => {
                    match stdout_result {
                        Ok(Some(line)) => {
                            if line.is_empty() { continue; }
                            debug!(session_id = %session.id, "Received from subprocess: {}", line);

                            // Echo STDOUT in green if colored output is enabled
                            if colored_output {
                                let timestamp = format!("[{}]", Local::now().format("%H:%M:%S%.3f"));
                                if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                                    let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| line.clone());
                                    eprintln!("{} {} {}\n{}", timestamp, format!("[{}]", &session.id[..8]).green(), "← STDOUT:".green(), pretty.green());
                                } else {
                                    eprintln!("{} {} {}\n{}", timestamp, format!("[{}]", &session.id[..8]).green(), "← STDOUT:".green(), line.green());
                                }
                            }

                            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                                // Check if it's a response to a pending request
                                if let Some(id) = value.get("id") {
                                    let id_str = if id.is_string() {
                                        id.as_str().unwrap().to_string()
                                    } else {
                                        id.to_string()
                                    };

                                    if let Some((_, sender)) = session.pending_requests.remove(&id_str) {
                                        let _ = sender.send(value);
                                        continue;
                                    }
                                }

                                // Not a response to a pending request - broadcast as SSE event
                                let _ = session.broadcast_tx.send(line);
                            } else {
                                warn!(session_id = %session.id, "Failed to parse JSON from subprocess: {}", line);
                            }
                        }
                        Ok(None) => {
                            warn!(session_id = %session.id, "Subprocess stdout closed - assuming crash or exit");
                            break;
                        }
                        Err(e) => {
                            error!(session_id = %session.id, "Failed to read stdout: {}", e);
                            break;
                        }
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
                        Ok(Some(line)) if !line.is_empty() => {
                            let timestamp = format!("[{}]", Local::now().format("%H:%M:%S%.3f"));
                            if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                                let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| line.clone());
                                eprintln!("{} {} {}\n{}", timestamp, format!("[{}]", &session.id[..8]).red(), "⚠ STDERR:".red(), pretty.red());
                            } else {
                                eprintln!("{} {} {}\n{}", timestamp, format!("[{}]", &session.id[..8]).red(), "⚠ STDERR:".red(), line.red());
                            }
                        }
                        Ok(Some(_)) => {} // Empty line
                        Ok(None) => {} // stderr closed
                        Err(e) => {
                            error!(session_id = %session.id, "Failed to read stderr: {}", e);
                        }
                    }
                }

                // Check for termination
                _ = async {
                    while !session.terminated.load(Ordering::SeqCst) {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                } => {
                    info!(session_id = %session.id, "Session terminated, stopping I/O handler");
                    break;
                }
            }
        }

        // Mark session as terminated if not already
        session.terminated.store(true, Ordering::SeqCst);

        // Send explicit error responses to all pending requests before clearing.
        // This prevents "Response channel closed" errors that manifest as cryptic
        // "Canceled: canceled" messages in clients.
        let pending_count = session.pending_requests.len();
        if pending_count > 0 {
            warn!(
                session_id = %session.id,
                pending_count = pending_count,
                "Session terminated with pending requests - sending error responses"
            );
            // Drain all pending requests and send error response
            let pending: Vec<_> = session
                .pending_requests
                .iter()
                .map(|entry| entry.key().clone())
                .collect();
            for id in pending {
                if let Some((_, sender)) = session.pending_requests.remove(&id) {
                    let error_response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32603,
                            "message": "Session terminated unexpectedly - subprocess may have crashed or handshake failed"
                        }
                    });
                    let _ = sender.send(error_response);
                }
            }
        }
    }
}
