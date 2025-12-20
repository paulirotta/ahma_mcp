//! Session management for HTTP bridge session isolation mode.
//!
//! Per R8D, session isolation allows multiple IDE instances to share a single
//! HTTP server with per-session sandbox scopes. Each session spawns a separate
//! `ahma_mcp` subprocess with its own sandbox scope derived from the client's
//! workspace roots.

use crate::error::{BridgeError, Result};
use dashmap::DashMap;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, broadcast, mpsc, oneshot},
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// MCP Root as defined in the protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRoot {
    /// URI of the root (must be file://)
    pub uri: String,
    /// Optional human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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

/// A single MCP session with its subprocess and state
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
    /// Stored initialize request for replay after subprocess restart
    initialize_request: Mutex<Option<Value>>,
    /// Termination reason (if terminated)
    termination_reason: Mutex<Option<SessionTerminationReason>>,
    /// Handle to the subprocess (for cleanup)
    child_handle: Mutex<Option<Child>>,
    /// Whether SSE is connected (client opened GET /mcp)
    sse_connected: AtomicBool,
    /// Whether initialized notification was received (MCP handshake complete)
    mcp_initialized: AtomicBool,
}

impl Session {
    /// Check if the session is terminated
    pub fn is_terminated(&self) -> bool {
        self.terminated.load(Ordering::SeqCst)
    }

    /// Check if the sandbox is locked
    pub fn is_sandbox_locked(&self) -> bool {
        self.sandbox_locked.load(Ordering::SeqCst)
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
    /// Returns true if roots/list_changed was sent.
    pub async fn mark_sse_connected(&self) -> Result<bool> {
        self.sse_connected.store(true, Ordering::SeqCst);
        debug!(session_id = %self.id, "SSE marked as connected");

        // If MCP is already initialized, send roots/list_changed now
        if self.mcp_initialized.load(Ordering::SeqCst) {
            self.send_roots_list_changed().await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Mark MCP as initialized (handshake complete) and trigger roots/list_changed if SSE is already connected.
    /// Returns true if roots/list_changed was sent.
    pub async fn mark_mcp_initialized(&self) -> Result<bool> {
        self.mcp_initialized.store(true, Ordering::SeqCst);
        debug!(session_id = %self.id, "MCP marked as initialized");

        // If SSE is already connected, send roots/list_changed now
        if self.sse_connected.load(Ordering::SeqCst) {
            self.send_roots_list_changed().await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Store the initialize request for replay after subprocess restart
    pub async fn store_initialize_request(&self, request: Value) {
        *self.initialize_request.lock().await = Some(request);
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

/// Configuration for session manager
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    /// Command to run the MCP server subprocess
    pub server_command: String,
    /// Base arguments to pass to the MCP server (before --sandbox-scope)
    pub server_args: Vec<String>,
    /// Default sandbox scope if client provides no roots
    pub default_scope: PathBuf,
    /// Enable colored terminal output for subprocess I/O
    pub enable_colored_output: bool,
}

/// Manages multiple MCP sessions, each with its own subprocess and sandbox scope
pub struct SessionManager {
    /// Active sessions indexed by session ID
    sessions: DashMap<String, Arc<Session>>,
    /// Configuration for spawning new sessions
    config: SessionManagerConfig,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: SessionManagerConfig) -> Self {
        Self {
            sessions: DashMap::new(),
            config,
        }
    }

    /// Create a new session, spawning a subprocess
    ///
    /// Returns the session ID to be returned to the client as `Mcp-Session-Id`
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
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr_mode)
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
            sse_connected: AtomicBool::new(false),
            mcp_initialized: AtomicBool::new(false),
            initialize_request: Mutex::new(None),
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
    pub async fn send_request(&self, session_id: &str, request: &Value) -> Result<Value> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        if session.is_terminated() {
            return Err(BridgeError::Communication(
                "Session has been terminated".to_string(),
            ));
        }

        // Get request ID
        let id_opt = request.get("id").map(|id| {
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
            match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
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

    /// Lock sandbox scope for a session (called when processing roots/list response)
    ///
    /// Per R8D.7-R8D.8, sandbox scope is determined from roots and cannot be changed.
    /// This method restarts the subprocess with the correct `--sandbox-scope` arguments
    /// to ensure file operations are validated against the client's workspace(s).
    ///
    /// Returns `true` if the subprocess was restarted (caller should NOT forward the
    /// roots/list response to the new subprocess, as it's already configured via CLI args
    /// and hasn't completed MCP handshake yet).
    pub async fn lock_sandbox(&self, session_id: &str, roots: &[McpRoot]) -> Result<bool> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        if session.sandbox_locked.load(Ordering::SeqCst) {
            // Already locked, no restart needed
            return Ok(false);
        }

        // Extract all roots as sandbox scopes, or use default if no roots
        let scopes: Vec<PathBuf> = roots
            .iter()
            .filter_map(|r| r.uri.strip_prefix("file://").map(PathBuf::from))
            .collect();

        let scopes = if scopes.is_empty() {
            vec![self.config.default_scope.clone()]
        } else {
            scopes
        };

        info!(
            session_id = %session_id,
            sandbox_scopes = ?scopes,
            "Locking sandbox scope(s) for session"
        );

        // Store the sandbox scopes
        *session.sandbox_scopes.lock().await = Some(scopes.clone());
        session.sandbox_locked.store(true, Ordering::SeqCst);

        // Restart subprocess with the correct sandbox scope(s)
        // This is necessary because the subprocess was started without knowing the client's workspace
        drop(session); // Release the lock before restart
        self.restart_session_with_sandbox(session_id, &scopes)
            .await?;

        // Subprocess was restarted - caller should not forward the roots response
        Ok(true)
    }

    /// Restart a session's subprocess with specific sandbox scopes.
    ///
    /// This terminates the existing subprocess and starts a new one with
    /// `--sandbox-scope <path>` arguments added for each scope.
    async fn restart_session_with_sandbox(
        &self,
        session_id: &str,
        sandbox_scopes: &[PathBuf],
    ) -> Result<()> {
        let session = self.sessions.get(session_id).ok_or_else(|| {
            BridgeError::Communication(format!("Session not found: {}", session_id))
        })?;

        info!(
            session_id = %session_id,
            sandbox_scopes = ?sandbox_scopes,
            "Restarting subprocess with sandbox scope(s)"
        );

        // Kill the existing subprocess
        if let Some(mut child) = session.child_handle.lock().await.take() {
            let _ = child.kill().await;
            info!(session_id = %session_id, "Killed existing subprocess");
        }

        // Build new args with --sandbox-scope for each scope
        let mut new_args = self.config.server_args.clone();
        for scope in sandbox_scopes {
            new_args.push("--sandbox-scope".to_string());
            new_args.push(scope.display().to_string());
        }

        // Spawn new subprocess with sandbox scope(s)
        let stderr_mode = if self.config.enable_colored_output {
            Stdio::piped()
        } else {
            Stdio::inherit()
        };

        let mut child = Command::new(&self.config.server_command)
            .args(&new_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr_mode)
            .spawn()
            .map_err(|e| {
                BridgeError::ServerProcess(format!(
                    "Failed to restart subprocess with sandbox: {}",
                    e
                ))
            })?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stderr = if self.config.enable_colored_output {
            child.stderr.take()
        } else {
            None
        };

        // Store the new child handle
        *session.child_handle.lock().await = Some(child);

        // Create new channels for the restarted subprocess
        let (tx, rx) = mpsc::channel::<String>(100);

        // Update the session's sender with the new channel
        *session.sender.lock().await = tx;

        // Spawn the I/O handler task for the new subprocess
        let session_clone = session.clone();
        let colored_output = self.config.enable_colored_output;
        tokio::spawn(async move {
            Self::handle_session_io(session_clone, rx, stdin, stdout, stderr, colored_output).await;
        });

        // Replay MCP handshake: send stored initialize request + initialized notification
        if let Some(init_request) = session.initialize_request.lock().await.as_ref() {
            info!(session_id = %session_id, "Replaying MCP handshake to restarted subprocess");

            // Send initialize request
            let json_str = serde_json::to_string(init_request).map_err(|e| {
                BridgeError::Communication(format!("Failed to serialize init request: {}", e))
            })?;
            session
                .sender
                .lock()
                .await
                .send(json_str)
                .await
                .map_err(|e| {
                    BridgeError::Communication(format!("Failed to send init request: {}", e))
                })?;

            // Give the subprocess a moment to process and respond
            // The I/O handler will receive the response and process it
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // Send initialized notification
            let initialized = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            });
            let json_str = serde_json::to_string(&initialized).map_err(|e| {
                BridgeError::Communication(format!("Failed to serialize initialized: {}", e))
            })?;
            session
                .sender
                .lock()
                .await
                .send(json_str)
                .await
                .map_err(|e| {
                    BridgeError::Communication(format!("Failed to send initialized: {}", e))
                })?;

            info!(session_id = %session_id, "MCP handshake replayed successfully");
        } else {
            warn!(session_id = %session_id, "No stored initialize request for handshake replay");
        }

        info!(
            session_id = %session_id,
            sandbox_scopes = ?sandbox_scopes,
            "Subprocess restarted successfully with sandbox scope(s)"
        );

        Ok(())
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
                        if let Ok(parsed) = serde_json::from_str::<Value>(&msg) {
                            let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| msg.clone());
                            eprintln!("{} {}\n{}", format!("[{}]", &session.id[..8]).cyan(), "→ STDIN:".cyan(), pretty.cyan());
                        } else {
                            eprintln!("{} {}\n{}", format!("[{}]", &session.id[..8]).cyan(), "→ STDIN:".cyan(), msg.cyan());
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
                Ok(Some(line)) = stdout_reader.next_line() => {
                    if line.is_empty() { continue; }
                    debug!(session_id = %session.id, "Received from subprocess: {}", line);

                    // Echo STDOUT in green if colored output is enabled
                    if colored_output {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                            let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| line.clone());
                            eprintln!("{} {}\n{}", format!("[{}]", &session.id[..8]).green(), "← STDOUT:".green(), pretty.green());
                        } else {
                            eprintln!("{} {}\n{}", format!("[{}]", &session.id[..8]).green(), "← STDOUT:".green(), line.green());
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
                            if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                                let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| line.clone());
                                eprintln!("{} {}\n{}", format!("[{}]", &session.id[..8]).red(), "⚠ STDERR:".red(), pretty.red());
                            } else {
                                eprintln!("{} {}\n{}", format!("[{}]", &session.id[..8]).red(), "⚠ STDERR:".red(), line.red());
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
        session.pending_requests.clear();
    }
}
