//! Shared test utilities for HTTP bridge integration tests.
//!
//! This module provides utilities for integration tests that need a running HTTP server.
//!
//! ## Dynamic Port Allocation
//!
//! Tests use dynamic port allocation (port 0) to avoid port conflicts when running
//! in parallel. Each test gets its own isolated server instance.
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
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;

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

    workspace_dir.join("target/debug/ahma_mcp")
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

    // Use the workspace .ahma/tools directory
    let tools_dir = workspace_dir.join(".ahma/tools");

    // Use a stable guidance file path (relative CWD is not stable in tests).
    let guidance_file = workspace_dir.join(".ahma").join("tool_guidance.json");

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
        "--guidance-file".to_string(),
        guidance_file.to_string_lossy().to_string(),
        "--sandbox-scope".to_string(),
        sandbox_scope.to_string_lossy().to_string(),
        "--log-to-stderr".to_string(),
    ];

    eprintln!("[TestServer] Starting test server with dynamic port");

    // Spawn with piped stderr to capture the bound port
    let mut child = Command::new(&binary)
        .args(&args)
        .current_dir(&workspace_dir)
        // SECURITY:
        // Don't enable permissive test mode for the server process.
        // Several integration tests rely on real sandbox/working_directory defaults.
        .env_remove("AHMA_TEST_MODE")
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_EXECUTION_MODE")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUST_TEST_THREADS")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn test server: {}", e))?;

    // Read stderr to find the bound port (AHMA_BOUND_PORT=<port>)
    let stderr = child.stderr.take().expect("Failed to capture stderr");
    let mut reader = BufReader::new(stderr);
    let mut port: Option<u16>;

    let start = Instant::now();
    let timeout = Duration::from_secs(10);

    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Timeout waiting for server to start".to_string());
        }

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF - process likely exited
                let _ = child.kill();
                let _ = child.wait();
                return Err("Server process exited before reporting port".to_string());
            }
            Ok(_) => {
                // Print server output for debugging
                eprint!("{}", line);

                // Look for the bound port marker
                if let Some(port_str) = line.strip_prefix("AHMA_BOUND_PORT=") {
                    port = port_str.trim().parse().ok();
                    if port.is_some() {
                        break;
                    }
                }
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Error reading server stderr: {}", e));
            }
        }
    }

    let bound_port = port.ok_or("Failed to parse bound port from server output")?;
    eprintln!("[TestServer] Server bound to port {}", bound_port);

    // Spawn a task to drain remaining stderr (prevent blocking)
    std::thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            eprintln!("{}", line);
        }
    });

    // Wait for server to be ready (health check)
    let client = Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", bound_port);

    for i in 0..50 {
        sleep(Duration::from_millis(100)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            eprintln!("[TestServer] Server ready after {}ms", (i + 1) * 100);
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
    Err("Test server failed to respond to health check within 5 seconds".to_string())
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

    /// Complete the MCP handshake: initialize + initialized notification + roots/list response.
    /// This MUST be called before any other MCP operations.
    ///
    /// Note: This uses a default empty roots list. For tests that need specific roots,
    /// use `initialize_with_roots()`.
    pub async fn initialize(&mut self) -> Result<JsonRpcResponse, String> {
        self.initialize_with_roots("mcp-test-client", &[]).await
    }

    /// Complete the MCP handshake with specific workspace roots.
    /// The roots are provided to the server in response to its roots/list request.
    pub async fn initialize_with_roots(
        &mut self,
        client_name: &str,
        roots: &[std::path::PathBuf],
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

        // Step 3: Handle roots/list request from server via SSE
        // The server sends roots/list to establish sandbox scope
        if let Some(ref session_id) = self.session_id {
            self.handle_roots_list_handshake(session_id, roots).await?;
        }

        Ok(init_response)
    }

    /// Handle the roots/list handshake over SSE.
    /// This listens for the server's roots/list request and responds with client roots.
    async fn handle_roots_list_handshake(
        &self,
        session_id: &str,
        roots: &[std::path::PathBuf],
    ) -> Result<(), String> {
        let url = format!("{}/mcp", self.base_url);

        // Open SSE stream to receive server requests
        let resp = self
            .client
            .get(&url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Mcp-Session-Id", session_id)
            .send()
            .await
            .map_err(|e| format!("Failed to open SSE stream: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("SSE stream HTTP {}", resp.status()));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err("Timeout waiting for roots/list from server".to_string());
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

                    // Extract data lines from SSE event
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

                    // Found roots/list request - respond with client roots
                    let id = value.get("id").cloned().ok_or("roots/list must have id")?;

                    let roots_json: Vec<Value> = roots
                        .iter()
                        .map(|p| {
                            json!({
                                "uri": format!("file://{}", p.display()),
                                "name": p.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("root")
                            })
                        })
                        .collect();

                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "roots": roots_json
                        }
                    });

                    // Send the roots/list response
                    let _ = self
                        .client
                        .post(self.mcp_url())
                        .header("Content-Type", "application/json")
                        .header("Mcp-Session-Id", session_id)
                        .json(&response)
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await
                        .map_err(|e| format!("Failed to send roots/list response: {}", e))?;

                    // Small delay to let server process roots
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    return Ok(());
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
