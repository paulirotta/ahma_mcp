//! HTTP Bridge Integration Tests
//!
//! These tests verify end-to-end HTTP bridge functionality by:
//! 1. Starting the HTTP bridge with a real ahma_mcp subprocess
//! 2. Sending requests through the HTTP interface
//! 3. Verifying correct responses
//!
//! These tests reproduce the bug where calling a tool from a different project
//! (different working_directory) fails with "expect initialized request" error.

use reqwest::Client;
use serde_json::{Value, json};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Find an available port for testing
fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to any port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
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

/// Start the HTTP bridge server and return the process and URL
async fn start_http_bridge(
    port: u16,
    tools_dir: &PathBuf,
    sandbox_scope: &PathBuf,
) -> std::process::Child {
    let binary = get_ahma_mcp_binary();

    let child = Command::new(&binary)
        .args([
            "--mode",
            "http",
            "--http-port",
            &port.to_string(),
            "--tools-dir",
            &tools_dir.to_string_lossy(),
            "--sandbox-scope",
            &sandbox_scope.to_string_lossy(),
        ])
        .env("AHMA_TEST_MODE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start HTTP bridge");

    // Wait for server to be ready
    let client = Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", port);

    for _ in 0..30 {
        sleep(Duration::from_millis(200)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success() {
                return child;
            }
    }

    panic!("HTTP bridge failed to start within timeout");
}

/// Send a JSON-RPC request to the MCP endpoint
async fn send_mcp_request(
    client: &Client,
    base_url: &str,
    request: &Value,
    session_id: Option<&str>,
) -> Result<(Value, Option<String>), String> {
    let url = format!("{}/mcp", base_url);

    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(30));

    if let Some(id) = session_id {
        req = req.header("Mcp-Session-Id", id);
    }

    let response = req
        .json(request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    // Debug: print all headers
    eprintln!(
        "Response headers for request {}:",
        request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
    );
    for (name, value) in response.headers().iter() {
        if name.as_str().eq_ignore_ascii_case("mcp-session-id") {
            eprintln!("  {}: <redacted>", name);
        } else {
            eprintln!("  {}: {:?}", name, value);
        }
    }

    // Get session ID from response header (case-insensitive)
    let new_session_id = response
        .headers()
        .get("mcp-session-id")
        .or_else(|| response.headers().get("Mcp-Session-Id"))
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, text));
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok((body, new_session_id))
}

/// Test: Calling a tool from a DIFFERENT working directory than the sandbox scope
///
/// This reproduces the bug where:
/// 1. HTTP server starts with sandbox_scope = /path/to/ahma_mcp  
/// 2. Client sends tools/call with working_directory = /path/to/different_project
/// 3. Server should handle this properly (either allowing cross-project calls via
///    the HTTP bridge's session isolation, or returning a proper sandbox error)
///
/// The BUG was: Server returns "expect initialized request" because the MCP
/// subprocess never received an initialize request.
#[tokio::test]
async fn test_tool_call_with_different_working_directory() {
    // Create temp directories for tools and sandbox scope
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Create a simple echo tool config
    let tool_config = json!({
        "name": "echo",
        "description": "Echo test tool",
        "command": "echo",
        "subcommands": [{
            "name": "default",
            "description": "Echo a message"
        }]
    });
    std::fs::write(
        tools_dir.join("echo.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    // Sandbox scope is the temp directory
    let sandbox_scope = temp_dir.path().to_path_buf();

    // A "different project" directory (still needs to exist for the test)
    let different_project = TempDir::new().expect("Failed to create different project dir");
    let different_project_path = different_project.path().to_path_buf();

    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    // Step 1: Send initialize request (no session ID - creates new session)
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    });

    let result = send_mcp_request(&client, &base_url, &init_request, None).await;

    let (init_response, session_id) = match result {
        Ok(r) => r,
        Err(e) => panic!("Initialize request failed: {}", e),
    };

    // Debug: print the response
    eprintln!("Initialize response: {:?}", init_response);
    // Session IDs should not be logged verbatim (CodeQL).
    eprintln!("Session ID from header: <redacted>");

    // In single-process mode (no --session-isolation), there's no session ID
    // Just verify initialize succeeded
    let session_id_for_requests = session_id;

    // Verify initialize response
    assert!(
        init_response.get("result").is_some(),
        "Initialize should return result, got: {:?}",
        init_response
    );

    // Step 2: Send initialized notification
    let initialized_notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });

    let _ = send_mcp_request(
        &client,
        &base_url,
        &initialized_notification,
        session_id_for_requests.as_deref(),
    )
    .await;
    // Notifications don't return responses, that's OK

    // Step 3: Call a tool with working_directory OUTSIDE the sandbox scope
    // This is the scenario that was failing with "expect initialized request"
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "echo_default",
            "arguments": {
                "working_directory": different_project_path.to_string_lossy()
            }
        }
    });

    let (tool_response, _) = send_mcp_request(
        &client,
        &base_url,
        &tool_call,
        session_id_for_requests.as_deref(),
    )
    .await
    .expect("Tool call should not fail with connection error");

    // The response should either:
    // - Succeed (if session isolation allows cross-project calls)
    // - Return a proper JSON-RPC error about sandbox violation
    // It should NOT return "expect initialized request" error

    let error_message = tool_response
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");

    assert!(
        !error_message.contains("expect initialized request"),
        "Should NOT get 'expect initialized request' error. Got: {:?}",
        tool_response
    );

    // Clean up
    server.kill().expect("Failed to kill server");
}

/// Test: Basic tool call within sandbox scope works correctly
#[tokio::test]
async fn test_basic_tool_call_within_sandbox() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Create echo tool config
    let tool_config = json!({
        "name": "echo",
        "description": "Echo test tool",
        "command": "echo",
        "subcommands": [{
            "name": "default",
            "description": "Echo a message"
        }]
    });
    std::fs::write(
        tools_dir.join("echo.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    let sandbox_scope = temp_dir.path().to_path_buf();
    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    // Initialize
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    });

    let (init_response, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .expect("Initialize should succeed");

    // Verify initialize succeeded
    assert!(
        init_response.get("result").is_some(),
        "Initialize should return result, got: {:?}",
        init_response
    );

    // In single-process mode, no session ID is returned - that's OK
    let session_id_for_requests = session_id;

    // Send initialized notification
    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(
        &client,
        &base_url,
        &initialized,
        session_id_for_requests.as_deref(),
    )
    .await;

    // Call tool WITHIN sandbox scope
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "echo_default",
            "arguments": {
                "working_directory": sandbox_scope.to_string_lossy()
            }
        }
    });

    let (response, _) = send_mcp_request(
        &client,
        &base_url,
        &tool_call,
        session_id_for_requests.as_deref(),
    )
    .await
    .expect("Tool call should succeed");

    // Should have result, not "expect initialized request" error
    let error = response.get("error");
    if let Some(err) = error {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
        assert!(
            !msg.contains("expect initialized request"),
            "Should NOT get 'expect initialized request' error. Got: {:?}",
            response
        );
    }

    server.kill().expect("Failed to kill server");
}

/// Test: Tool call WITHOUT initialize should fail with proper error
///
/// This test reproduces the bug where the HTTP bridge allows a tools/call
/// to be sent before initialize, causing the subprocess to error with
/// "expect initialized request".
///
/// The expected behavior: The HTTP bridge should reject requests that come
/// before initialize, OR handle initialization automatically.
#[tokio::test]
async fn test_tool_call_without_initialize_returns_proper_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Create echo tool config
    let tool_config = json!({
        "name": "echo",
        "description": "Echo test tool",
        "command": "echo",
        "subcommands": [{
            "name": "default",
            "description": "Echo a message"
        }]
    });
    std::fs::write(
        tools_dir.join("echo.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    let sandbox_scope = temp_dir.path().to_path_buf();
    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    // SKIP initialize - send tools/call directly
    // This reproduces the user's bug where the subprocess gets a tools/call first
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "echo_default",
            "arguments": {
                "working_directory": sandbox_scope.to_string_lossy()
            }
        }
    });

    let result = send_mcp_request(&client, &base_url, &tool_call, None).await;

    eprintln!("Tool call without initialize result: {:?}", result);

    // This SHOULD fail - but the question is HOW it fails
    // Good: HTTP 400 or JSON-RPC error saying "not initialized" or similar
    // Bad: "expect initialized request" (means subprocess crashed)

    // Check HTTP response
    let response_error_msg = match &result {
        Ok((response, _)) => response
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string(),
        Err(e) => e.clone(),
    };

    // Kill server and capture stderr to check for "expect initialized request"
    server.kill().expect("Failed to kill server");

    // Read stderr from the server process
    let stderr_output = if let Some(stderr) = server.stderr.take() {
        use std::io::Read;
        let mut buf = String::new();
        std::io::BufReader::new(stderr)
            .read_to_string(&mut buf)
            .unwrap_or(0);
        buf
    } else {
        String::new()
    };

    eprintln!("Server stderr: {}", stderr_output);

    // The CRITICAL check: If stderr contains "expect initialized request", the bug exists
    // This means the subprocess received a request before initialization
    assert!(
        !stderr_output.contains("expect initialized request"),
        "BUG REPRODUCED: Server subprocess received 'expect initialized request' error.\n\
         This means the HTTP bridge is forwarding requests to an uninitialized subprocess.\n\
         Stderr: {}",
        stderr_output
    );

    // Also check HTTP response doesn't contain this error
    assert!(
        !response_error_msg.contains("expect initialized request"),
        "BUG: HTTP response contains 'expect initialized request': {}",
        response_error_msg
    );
}
