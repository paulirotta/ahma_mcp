//! Shared test utilities for HTTP bridge integration tests.
//!
//! This module provides utilities for integration tests that need a running HTTP server.
//!
//! ## Port Assignment
//!
//! - **Production default**: Port 3000
//! - **Integration tests**: Port 5721 (reserved for shared test server)
//!
//! ## Usage Patterns
//!
//! 1. **Tests using the shared singleton** (sse_tool_integration_test, http_roots_handshake_test):
//!    Call `get_test_server().await` to get a shared server on port 5721.
//!
//! 2. **Tests needing custom configurations** (http_bridge_integration_test):
//!    Use dynamic port allocation via `find_available_port()` and spawn their own server.
//!
//! ## MCP Protocol Helpers
//!
//! Use `McpTestClient` for proper MCP protocol handling:
//! ```ignore
//! let _server = get_test_server().await;
//! let mut client = McpTestClient::new();
//! client.initialize().await.expect("handshake failed");
//! let result = client.call_tool("python", json!({"subcommand": "version"})).await;
//! ```

// Allow dead_code - these are test utilities, and rustc can't see usage across test crates
#![allow(dead_code)]

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;

/// Reserved port for integration tests. DO NOT use port 3000 in tests.
pub const AHMA_INTEGRATION_TEST_SERVER_PORT: u16 = 5721;

/// A running test server instance
struct TestServerInstance {
    child: Child,
    _temp_dir: TempDir, // Keep alive for the duration of the server
}

impl Drop for TestServerInstance {
    fn drop(&mut self) {
        eprintln!(
            "[TestServer] Shutting down test server on port {}",
            AHMA_INTEGRATION_TEST_SERVER_PORT
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Guard that provides access to the test server
pub struct TestServerGuard {
    _inner: Arc<Mutex<Option<TestServerInstance>>>,
}

impl TestServerGuard {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", AHMA_INTEGRATION_TEST_SERVER_PORT)
    }

    pub fn port(&self) -> u16 {
        AHMA_INTEGRATION_TEST_SERVER_PORT
    }
}

/// Global singleton for the test server (within this process)
static TEST_SERVER: OnceLock<Arc<Mutex<Option<TestServerInstance>>>> = OnceLock::new();

/// Install panic hook to clean up server on panic
fn install_panic_hook() {
    static HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
    HOOK_INSTALLED.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Try to clean up the test server
            if let Some(server) = TEST_SERVER.get()
                && let Ok(mut guard) = server.lock()
                && let Some(mut instance) = guard.take()
            {
                eprintln!("[TestServer] Panic detected! Cleaning up test server...");
                let _ = instance.child.kill();
                let _ = instance.child.wait();
            }
            default_hook(info);
        }));
    });
}

/// Check if the test server is already running (started by another test process)
async fn is_server_running() -> bool {
    let client = Client::new();
    let health_url = format!(
        "http://127.0.0.1:{}/health",
        AHMA_INTEGRATION_TEST_SERVER_PORT
    );

    let ok = match client
        .get(&health_url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    };

    if !ok {
        return false;
    }

    // Health can be OK while /mcp is wedged (e.g. stuck session subprocess / I/O handler).
    // Probe initialize with a short timeout to ensure the server is actually usable.
    let init_url = format!("http://127.0.0.1:{}/mcp", AHMA_INTEGRATION_TEST_SERVER_PORT);
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"roots": {}},
            "clientInfo": {"name": "health-probe", "version": "0"}
        }
    });
    match client
        .post(&init_url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(&init_req)
        .timeout(Duration::from_millis(800))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
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

/// Start the test server
async fn start_test_server() -> Result<TestServerInstance, String> {
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

    // Build command args
    let args = vec![
        "--mode".to_string(),
        "http".to_string(),
        "--http-port".to_string(),
        AHMA_INTEGRATION_TEST_SERVER_PORT.to_string(),
        "--sync".to_string(),
        "--tools-dir".to_string(),
        tools_dir.to_string_lossy().to_string(),
        "--guidance-file".to_string(),
        guidance_file.to_string_lossy().to_string(),
        "--sandbox-scope".to_string(),
        sandbox_scope.to_string_lossy().to_string(),
        "--log-to-stderr".to_string(),
    ];

    eprintln!(
        "[TestServer] Starting test server on port {}",
        AHMA_INTEGRATION_TEST_SERVER_PORT
    );

    let child = Command::new(&binary)
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
        // Avoid deadlock: do not pipe output unless we actively drain it.
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to spawn test server: {}", e))?;

    // Wait for server to be ready
    let client = Client::new();
    let health_url = format!(
        "http://127.0.0.1:{}/health",
        AHMA_INTEGRATION_TEST_SERVER_PORT
    );

    for i in 0..50 {
        sleep(Duration::from_millis(100)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            eprintln!("[TestServer] Server ready after {}ms", (i + 1) * 100);
            return Ok(TestServerInstance {
                child,
                _temp_dir: temp_dir,
            });
        }
    }

    // Failed to start - clean up
    let mut child = child;
    let _ = child.kill();
    let _ = child.wait();
    Err("Test server failed to start within 5 seconds".to_string())
}

/// Get or start the shared test server.
///
/// This function ensures a test server is running on port 5721:
/// 1. If a server is already running (from this or another process), use it
/// 2. If not, start one and keep it running for subsequent tests
///
/// Note: Due to nextest running tests in separate processes, each process
/// may start its own server. This is OK - the first one to bind wins.
///
/// For cargo test (single process, multiple threads), we use a tokio OnceCell
/// to ensure server startup happens only once.
pub async fn get_test_server() -> TestServerGuard {
    install_panic_hook();

    // Use a static OnceCell to ensure we only try to start once per process
    static INIT: tokio::sync::OnceCell<Arc<Mutex<Option<TestServerInstance>>>> =
        tokio::sync::OnceCell::const_new();

    let server_arc = INIT
        .get_or_init(|| async {
            // First check if a server is already running (from another process)
            if is_server_running().await {
                eprintln!(
                    "[TestServer] Server already running on port {}, reusing",
                    AHMA_INTEGRATION_TEST_SERVER_PORT
                );
                return Arc::new(Mutex::new(None));
            }

            // Try to start a new server
            match start_test_server().await {
                Ok(instance) => Arc::new(Mutex::new(Some(instance))),
                Err(e) => {
                    // If we failed because the port is in use, another process grabbed it
                    // Check if a server is now running
                    sleep(Duration::from_millis(500)).await;
                    if is_server_running().await {
                        eprintln!(
                            "[TestServer] Another process started server on port {}, reusing",
                            AHMA_INTEGRATION_TEST_SERVER_PORT
                        );
                        Arc::new(Mutex::new(None))
                    } else {
                        panic!("Failed to start test server: {}", e);
                    }
                }
            }
        })
        .await;

    TestServerGuard {
        _inner: server_arc.clone(),
    }
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
/// let mut client = McpTestClient::new();
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
    /// Create a new MCP test client for the shared test server
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{}", AHMA_INTEGRATION_TEST_SERVER_PORT),
            session_id: None,
        }
    }

    /// Create a new MCP test client for a custom URL
    pub fn with_url(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            session_id: None,
        }
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

impl Default for McpTestClient {
    fn default() -> Self {
        Self::new()
    }
}
