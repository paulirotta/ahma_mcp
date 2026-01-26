//! Shared test utilities for HTTP bridge integration tests.
//!
//! This module provides utilities for integration tests that need a running HTTP server.
//!
//! ## Dynamic Port Allocation
//!
//! Tests use dynamic port allocation (port 0) to avoid port conflicts when running
//! in parallel. Each test gets its own isolated server instance.
//!
//! ## Sandbox Isolation Testing
//!
//! When testing real sandbox behavior (not bypassed via test mode), use
//! `SandboxTestEnv` to ensure spawned `ahma_mcp` processes don't inherit
//! environment variables that enable permissive test mode.
//!
//! ## Usage
//!
//! ```ignore
//! let server = spawn_test_server().await.expect("Failed to spawn server");
//! let mut client = McpTestClient::with_url(&server.base_url());
//! client.initialize().await.expect("handshake failed");
//! let result = client.call_tool("python", json!({"subcommand": "version"})).await;
//! // Server is automatically cleaned up when `server` is dropped
//! ```

// Allow dead_code - these are test utilities, and rustc can't see usage across test crates
#![allow(dead_code)]

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::env;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;

// =============================================================================
// Sandbox Test Environment Helper
// =============================================================================

/// Environment variables that enable permissive test mode in ahma_mcp.
///
/// When spawning ahma_mcp processes for tests that verify real sandbox behavior,
/// these env vars must be cleared to prevent the process from auto-enabling
/// a permissive test mode that bypasses sandbox validation.
///
/// See AGENTS.md for the full explanation of this failure mode.
pub const SANDBOX_BYPASS_ENV_VARS: &[&str] = &[
    "AHMA_TEST_MODE",
    "NEXTEST",
    "NEXTEST_EXECUTION_MODE",
    "CARGO_TARGET_DIR",
    "RUST_TEST_THREADS",
];

/// Helper to configure a `Command` for sandbox-isolated testing.
///
/// Removes all environment variables that could trigger permissive test mode
/// in the spawned ahma_mcp process, ensuring real sandbox behavior is tested.
///
/// # Usage
/// ```ignore
/// let mut cmd = Command::new(&binary_path);
/// SandboxTestEnv::configure(&mut cmd);
/// // cmd is now configured to test real sandbox behavior
/// ```
pub struct SandboxTestEnv;

impl SandboxTestEnv {
    /// Configure a Command to test real sandbox behavior by removing bypass env vars.
    pub fn configure(cmd: &mut Command) -> &mut Command {
        for var in SANDBOX_BYPASS_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

    /// Configure a tokio Command to test real sandbox behavior.
    pub fn configure_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
        for var in SANDBOX_BYPASS_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

    /// Get a list of key=value pairs for the env vars that would bypass sandbox.
    /// Useful for debugging which vars are set in the current environment.
    pub fn current_bypass_vars() -> Vec<String> {
        SANDBOX_BYPASS_ENV_VARS
            .iter()
            .filter_map(|var| {
                std::env::var(var)
                    .ok()
                    .map(|val| format!("{}={}", var, val))
            })
            .collect()
    }

    /// Check if any bypass env vars are currently set.
    pub fn is_bypass_active() -> bool {
        SANDBOX_BYPASS_ENV_VARS
            .iter()
            .any(|var| std::env::var(var).is_ok())
    }
}

// =============================================================================
// File URI Utilities
// =============================================================================

/// Parse a file:// URI to a filesystem path.
///
/// Handles:
/// - Standard file:// URIs
/// - URL-encoded characters (%20 for space, etc.)
/// - Missing file:// prefix (returns None)
///
/// # Errors
/// Returns None for:
/// - Non-file:// URIs (http://, https://, etc.)
/// - Malformed URIs that can't be decoded
/// - Empty URIs
pub fn parse_file_uri(uri: &str) -> Option<PathBuf> {
    if !uri.starts_with("file://") {
        return None;
    }
    let path_str = uri.strip_prefix("file://")?;
    if path_str.is_empty() {
        return None;
    }
    // URL-decode the path
    let decoded = urlencoding::decode(path_str).ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

/// Encode a filesystem path as a file:// URI.
///
/// Properly encodes special characters like spaces, unicode, etc.
pub fn encode_file_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    // Encode everything except unreserved chars and /
    let mut out = String::with_capacity(path_str.len() + 7);
    out.push_str("file://");
    for &b in path_str.as_bytes() {
        let keep = matches!(
            b,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
        );
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

/// Malformed URI test cases for edge case testing.
pub mod malformed_uris {
    /// URIs that should be rejected (return None from parse_file_uri)
    pub const INVALID: &[&str] = &[
        "",                         // Empty
        "file://",                  // No path (missing slash after authority)
        "http://localhost/path",    // Wrong scheme
        "https://example.com/file", // Wrong scheme
        "ftp://server/file",        // Wrong scheme
        "file:",                    // Incomplete
        "file:/",                   // Incomplete (only one slash)
    ];

    /// URIs that might look valid but have edge cases
    pub const EDGE_CASES: &[(&str, Option<&str>)] = &[
        ("file:///tmp/test", Some("/tmp/test")), // Triple slash (valid)
        ("file:///", Some("/")),                 // Root directory (valid)
        ("file:///tmp/test%20file", Some("/tmp/test file")), // URL-encoded space
        ("file:///tmp/%E2%9C%93", Some("/tmp/✓")), // URL-encoded unicode
        ("file:///tmp/a%2Fb", Some("/tmp/a/b")), // Encoded slash (debatable)
        ("file:///C:/Windows", Some("/C:/Windows")), // Windows-style path on Unix
    ];
}

// =============================================================================
// Test Server Infrastructure (existing code continues below)
// =============================================================================

/// Legacy constant for backward compatibility. Prefer using `spawn_test_server()` instead.
#[deprecated(note = "Use spawn_test_server() for dynamic port allocation")]
pub const AHMA_INTEGRATION_TEST_SERVER_PORT: u16 = 5721;

/// A running test server instance with dynamic port
pub struct TestServerInstance {
    child: Child,
    port: u16,
    _temp_dir: TempDir, // Keep alive for the duration of the server
}

impl TestServerInstance {
    /// Get the base URL for this server
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Get the port this server is listening on
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for TestServerInstance {
    fn drop(&mut self) {
        eprintln!(
            "[TestServer] Shutting down test server on port {}",
            self.port
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Build the ahma_mcp binary if needed and return the path
fn get_ahma_mcp_binary() -> PathBuf {
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();

    // Build ahma_mcp binary
    let output = Command::new("cargo")
        .current_dir(&workspace_dir)
        .args(["build", "--package", "ahma_core", "--bin", "ahma_mcp"])
        .output()
        .expect("Failed to run cargo build");

    assert!(
        output.status.success(),
        "Failed to build ahma_mcp: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check for CARGO_TARGET_DIR to support tools like llvm-cov
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_dir.join("target"));

    target_dir.join("debug/ahma_mcp")
}

/// Spawn a new test server with dynamic port allocation.
///
/// Each call creates an isolated server instance on a randomly assigned port.
/// The server is automatically killed when the returned `TestServerInstance` is dropped.
///
/// # Example
/// ```ignore
/// let server = spawn_test_server().await.expect("Failed to spawn server");
/// println!("Server running on port {}", server.port());
/// // Use server.base_url() to connect
/// // Server stops when `server` goes out of scope
/// ```
pub async fn spawn_test_server() -> Result<TestServerInstance, String> {
    let binary = get_ahma_mcp_binary();

    // Get the workspace directory
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();

    // Use the workspace .ahma directory
    let tools_dir = workspace_dir.join(".ahma");

    // Create temp directory for sandbox scope
    let temp_dir = TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let sandbox_scope = temp_dir.path().to_path_buf();

    // Build command args - use port 0 for dynamic allocation
    let args = vec![
        "--mode".to_string(),
        "http".to_string(),
        "--http-port".to_string(),
        "0".to_string(), // Dynamic port allocation
        "--sync".to_string(),
        "--tools-dir".to_string(),
        tools_dir.to_string_lossy().to_string(),
        "--sandbox-scope".to_string(),
        sandbox_scope.to_string_lossy().to_string(),
        "--log-to-stderr".to_string(),
    ];

    eprintln!("[TestServer] Starting test server with dynamic port");

    // Spawn with piped stderr to capture the bound port
    let mut cmd = Command::new(&binary);
    cmd.args(&args)
        .current_dir(&workspace_dir)
        // SECURITY:
        // Don't enable permissive test mode for the server process.
        // Several integration tests rely on real sandbox/working_directory defaults.
        .env_remove("AHMA_TEST_MODE")
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_EXECUTION_MODE")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUST_TEST_THREADS")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Explicitly propagate handshake timeout so tests can shrink it reliably
    if let Ok(timeout) = env::var("AHMA_HANDSHAKE_TIMEOUT_SECS") {
        cmd.env("AHMA_HANDSHAKE_TIMEOUT_SECS", timeout);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn test server: {}", e))?;

    // Read stdout/stderr to find the bound port (AHMA_BOUND_PORT=<port>)
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");
    let (line_tx, line_rx) = std::sync::mpsc::channel::<String>();

    let tx_out = line_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_out.send(line);
        }
    });

    let tx_err = line_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_err.send(line);
        }
    });

    let start = Instant::now();
    let timeout = Duration::from_secs(30);
    let mut bound_port: Option<u16> = None;

    while start.elapsed() <= timeout {
        match line_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                eprintln!("{}", line);
                if let Some(idx) = line.find("AHMA_BOUND_PORT=") {
                    let port_str = &line[idx + "AHMA_BOUND_PORT=".len()..];
                    bound_port = port_str.trim().parse().ok();
                    if bound_port.is_some() {
                        break;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let bound_port = match bound_port {
        Some(port) => port,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Timeout waiting for server to start".to_string());
        }
    };

    eprintln!("[TestServer] Server bound to port {}", bound_port);

    // Wait for server to be ready (health check)
    let client = Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", bound_port);

    let health_start = Instant::now();
    let health_timeout = Duration::from_secs(15);

    while health_start.elapsed() <= health_timeout {
        sleep(Duration::from_millis(100)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            eprintln!(
                "[TestServer] Server ready after {}ms",
                health_start.elapsed().as_millis()
            );
            return Ok(TestServerInstance {
                child,
                port: bound_port,
                _temp_dir: temp_dir,
            });
        }
    }

    // Failed to start - clean up
    let _ = child.kill();
    let _ = child.wait();
    Err("Test server failed to respond to health check within 15 seconds".to_string())
}

/// Legacy function for backward compatibility. Spawns a new server each time.
#[deprecated(note = "Use spawn_test_server() directly")]
pub async fn get_test_server() -> TestServerInstance {
    spawn_test_server()
        .await
        .expect("Failed to spawn test server")
}

// =============================================================================
// MCP Protocol Test Utilities
// =============================================================================

/// Atomic request ID counter for JSON-RPC
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Get next request ID
fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// JSON-RPC request structure
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    /// Create a new request with auto-generated ID
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(next_request_id()),
            method: method.to_string(),
            params,
        }
    }

    /// Create a notification (no ID, no response expected)
    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        }
    }

    /// Create an initialize request
    pub fn initialize(client_name: &str) -> Self {
        Self::new(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"roots": {}},
                "clientInfo": {"name": client_name, "version": "1.0"}
            }),
        )
    }

    /// Create the initialized notification (required after initialize response)
    pub fn initialized() -> Self {
        Self::notification("notifications/initialized", json!({}))
    }

    /// Create a tools/call request
    pub fn call_tool(name: &str, arguments: Value) -> Self {
        Self::new(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments
            }),
        )
    }

    /// Create a tools/list request
    pub fn list_tools() -> Self {
        Self::new("tools/list", json!({}))
    }
}

/// JSON-RPC response structure
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Check if this response indicates success
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    /// Get the error message if present
    pub fn error_message(&self) -> Option<String> {
        self.error.as_ref().map(|e| e.message.clone())
    }

    /// Extract text content from a tools/call result
    pub fn extract_tool_output(&self) -> Option<String> {
        self.result.as_ref().and_then(|r| {
            r.get("content").and_then(|c| c.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
    }
}

/// JSON-RPC error structure
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Result of a tool call
#[derive(Debug)]
pub struct ToolCallResult {
    pub tool_name: String,
    pub success: bool,
    pub duration_ms: u128,
    pub error: Option<String>,
    pub output: Option<String>,
}

/// MCP test client that handles the protocol correctly.
///
/// This client:
/// - Manages session IDs automatically
/// - Completes the MCP handshake properly (initialize + initialized notification)
/// - Provides convenient methods for common operations
///
/// # Example
/// ```ignore
/// let _server = get_test_server().await;
/// let mut client = McpTestClient::with_url(&server.base_url());
/// client.initialize().await.expect("handshake failed");
/// let result = client.call_tool("file_tools", json!({"subcommand": "pwd"})).await;
/// assert!(result.success);
/// ```
pub struct McpTestClient {
    client: Client,
    base_url: String,
    session_id: Option<String>,
}

impl McpTestClient {
    /// Create a new MCP test client for a specific server URL.
    /// Use `with_url(&server.base_url())` where `server` is from `spawn_test_server()`.
    pub fn with_url(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            session_id: None,
        }
    }

    /// Create a new MCP test client from a TestServerInstance
    pub fn for_server(server: &TestServerInstance) -> Self {
        Self::with_url(&server.base_url())
    }

    /// Get the MCP endpoint URL
    fn mcp_url(&self) -> String {
        format!("{}/mcp", self.base_url)
    }

    /// Complete the MCP handshake: initialize + initialized notification.
    /// This MUST be called before any other MCP operations.
    pub async fn initialize(&mut self) -> Result<JsonRpcResponse, String> {
        self.initialize_with_name("mcp-test-client").await
    }

    /// Complete the MCP handshake with a custom client name
    pub async fn initialize_with_name(
        &mut self,
        client_name: &str,
    ) -> Result<JsonRpcResponse, String> {
        // Step 1: Send initialize request
        let init_request = JsonRpcRequest::initialize(client_name);
        let response = self
            .client
            .post(self.mcp_url())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&init_request)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Initialize request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Initialize failed with HTTP {}: {}", status, text));
        }

        // Extract session ID from response headers
        self.session_id = response
            .headers()
            .get("mcp-session-id")
            .or_else(|| response.headers().get("Mcp-Session-Id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let init_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse initialize response: {}", e))?;

        if init_response.error.is_some() {
            return Err(format!(
                "Initialize returned error: {:?}",
                init_response.error
            ));
        }

        // Step 2: Send initialized notification (required by MCP protocol)
        let initialized_notification = JsonRpcRequest::initialized();
        let _ = self
            .client
            .post(self.mcp_url())
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", self.session_id.as_deref().unwrap_or(""))
            .json(&initialized_notification)
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        Ok(init_response)
    }

    /// Complete the MCP handshake with roots to lock sandbox scope.
    ///
    /// This performs the full VS Code-style handshake:
    /// 1. Send initialize request
    /// 2. Open SSE connection
    /// 3. Wait for roots/list request over SSE
    /// 4. Respond with the provided roots
    /// 5. Return after sandbox is locked
    pub async fn initialize_with_roots(
        &mut self,
        client_name: &str,
        roots: &[PathBuf],
    ) -> Result<JsonRpcResponse, String> {
        // Step 1: Send initialize request
        let init_request = JsonRpcRequest::initialize(client_name);
        let response = self
            .client
            .post(self.mcp_url())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&init_request)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Initialize request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Initialize failed with HTTP {}: {}", status, text));
        }

        // Extract session ID from response headers
        self.session_id = response
            .headers()
            .get("mcp-session-id")
            .or_else(|| response.headers().get("Mcp-Session-Id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let init_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse initialize response: {}", e))?;

        if init_response.error.is_some() {
            return Err(format!(
                "Initialize returned error: {:?}",
                init_response.error
            ));
        }

        let session_id = self.session_id.clone().ok_or("No session ID received")?;

        // Step 2: Send initialized notification
        let initialized_notification = JsonRpcRequest::initialized();
        let _ = self
            .client
            .post(self.mcp_url())
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", &session_id)
            .json(&initialized_notification)
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        // Step 3: Open SSE connection and wait for roots/list
        let sse_resp = self
            .client
            .get(self.mcp_url())
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Mcp-Session-Id", &session_id)
            .send()
            .await
            .map_err(|e| format!("SSE connection failed: {}", e))?;

        if !sse_resp.status().is_success() {
            return Err(format!("SSE stream failed with HTTP {}", sse_resp.status()));
        }

        let mut stream = sse_resp.bytes_stream();
        let mut buffer = String::new();
        let deadline = Instant::now() + Duration::from_secs(10);

        loop {
            if Instant::now() > deadline {
                return Err("Timeout waiting for roots/list over SSE".to_string());
            }

            let chunk = tokio::time::timeout(Duration::from_millis(500), stream.next())
                .await
                .ok()
                .flatten();

            if let Some(next) = chunk {
                let bytes = next.map_err(|e| format!("SSE read error: {}", e))?;
                let text = String::from_utf8_lossy(&bytes);
                buffer.push_str(&text);

                // SSE events are separated by blank lines
                while let Some(idx) = buffer.find("\n\n") {
                    let raw_event = buffer[..idx].to_string();
                    buffer = buffer[idx + 2..].to_string();

                    let mut data_lines: Vec<&str> = Vec::new();
                    for line in raw_event.lines() {
                        let line = line.trim_end_matches('\r');
                        if let Some(rest) = line.strip_prefix("data:") {
                            data_lines.push(rest.trim());
                        }
                    }

                    if data_lines.is_empty() {
                        continue;
                    }

                    let data = data_lines.join("\n");
                    let Ok(value) = serde_json::from_str::<Value>(&data) else {
                        continue;
                    };

                    let method = value.get("method").and_then(|m| m.as_str());
                    if method != Some("roots/list") {
                        continue;
                    }

                    let id = value
                        .get("id")
                        .cloned()
                        .ok_or("roots/list must include id")?;

                    // Build roots response
                    let roots_json: Vec<Value> = roots
                        .iter()
                        .map(|p| {
                            json!({
                                "uri": format!("file://{}", p.display()),
                                "name": p.file_name().and_then(|n| n.to_str()).unwrap_or("root")
                            })
                        })
                        .collect();

                    let roots_response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "roots": roots_json
                        }
                    });

                    // Send roots/list response
                    let _ = self
                        .client
                        .post(self.mcp_url())
                        .header("Content-Type", "application/json")
                        .header("Mcp-Session-Id", &session_id)
                        .json(&roots_response)
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await
                        .map_err(|e| format!("Failed to send roots response: {}", e))?;

                    // Give the server a moment to lock sandbox
                    sleep(Duration::from_millis(100)).await;

                    return Ok(init_response);
                }
            }
        }
    }

    /// Send a raw JSON-RPC request with session handling
    pub async fn send_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
        let mut req_builder = self
            .client
            .post(self.mcp_url())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Some(ref session_id) = self.session_id {
            req_builder = req_builder.header("Mcp-Session-Id", session_id);
        }

        let response = req_builder
            .json(request)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("HTTP {}: {}", status, text));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Call a tool and return the result
    pub async fn call_tool(&self, name: &str, arguments: Value) -> ToolCallResult {
        let start = Instant::now();
        let request = JsonRpcRequest::call_tool(name, arguments);

        match self.send_request(&request).await {
            Ok(response) => {
                let duration_ms = start.elapsed().as_millis();

                if let Some(ref error) = response.error {
                    ToolCallResult {
                        tool_name: name.to_string(),
                        success: false,
                        duration_ms,
                        error: Some(format!("[{}] {}", error.code, error.message)),
                        output: None,
                    }
                } else {
                    ToolCallResult {
                        tool_name: name.to_string(),
                        success: true,
                        duration_ms,
                        error: None,
                        output: response.extract_tool_output(),
                    }
                }
            }
            Err(e) => ToolCallResult {
                tool_name: name.to_string(),
                success: false,
                duration_ms: start.elapsed().as_millis(),
                error: Some(e),
                output: None,
            },
        }
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Vec<Value>, String> {
        let request = JsonRpcRequest::list_tools();
        let response = self.send_request(&request).await?;

        response
            .result
            .and_then(|r| r.get("tools").cloned())
            .and_then(|t| t.as_array().cloned())
            .ok_or_else(|| "No tools array in response".to_string())
    }

    /// Check if a specific tool is available
    pub async fn is_tool_available(&self, tool_name: &str) -> bool {
        match self.list_tools().await {
            Ok(tools) => tools.iter().any(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n == tool_name)
                    .unwrap_or(false)
            }),
            Err(_) => false,
        }
    }

    /// Get the current session ID (if initialized)
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Check if the client has been initialized
    pub fn is_initialized(&self) -> bool {
        self.session_id.is_some()
    }
}
