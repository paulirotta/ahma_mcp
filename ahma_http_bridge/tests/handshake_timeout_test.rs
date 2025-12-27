//! Tests for handshake timeout behavior.
//!
//! These tests verify that the HTTP bridge provides actionable error messages
//! when the MCP handshake fails to complete within the timeout period.
//!
//! The handshake requires:
//! 1. Client sends POST /mcp with initialize request → server returns session ID
//! 2. Client opens GET /mcp SSE stream with session header
//! 3. Client sends POST /mcp with notifications/initialized
//! 4. Server sends roots/list request over SSE
//! 5. Client responds with roots → sandbox locks
//!
//! If any step is missing, tools/call should fail with an actionable error.

mod common;

use common::spawn_test_server;
use reqwest::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{Value, json};
use std::time::Duration;

const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Test that tools/call without completing handshake returns a clear timeout error.
/// This simulates a broken client that sends initialize but never opens SSE.
#[tokio::test]
async fn test_tools_call_without_sse_returns_handshake_timeout() {
    // Use a very short timeout for this test
    // SAFETY: This test runs in isolation and controls the env var
    unsafe { std::env::set_var("AHMA_HANDSHAKE_TIMEOUT_SECS", "1") };

    let server = spawn_test_server()
        .await
        .expect("Failed to spawn test server");
    let client = Client::new();

    // Step 1: Initialize (creates session)
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": { "roots": {} },
            "clientInfo": { "name": "handshake_timeout_test", "version": "0.1" }
        }
    });

    let init_resp = client
        .post(format!("{}/mcp", server.base_url()))
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .json(&init_req)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("initialize POST failed");

    assert!(init_resp.status().is_success(), "Initialize should succeed");

    let session_id = init_resp
        .headers()
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .expect("Should have session ID")
        .to_string();

    // Intentionally skip: SSE connection, initialized notification, roots/list response

    // Wait for handshake timeout (1 second + buffer)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Try to call a tool - should get handshake timeout error
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "cargo",
            "arguments": { "subcommand": "locate-project" }
        }
    });

    let resp = client
        .post(format!("{}/mcp", server.base_url()))
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(MCP_SESSION_ID_HEADER, &session_id)
        .json(&tool_call)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("tools/call POST failed");

    // Should return 504 Gateway Timeout (not 409 Conflict)
    assert_eq!(
        resp.status().as_u16(),
        504,
        "Expected HTTP 504 for handshake timeout"
    );

    let body: Value = resp.json().await.expect("Response should be JSON");
    let error = body.get("error").expect("Should have error");

    // Verify error code is -32002 (handshake timeout)
    assert_eq!(
        error.get("code").and_then(|c| c.as_i64()),
        Some(-32002),
        "Expected error code -32002 for handshake timeout"
    );

    // Verify error message is actionable
    let msg = error
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("Handshake timeout") && msg.contains("SSE"),
        "Error message should mention handshake timeout and SSE. Got: {msg}"
    );
    assert!(
        msg.contains("SSE connected: false"),
        "Error should indicate SSE not connected. Got: {msg}"
    );
}

/// Test that tools/call without initialized notification returns timeout error.
/// This simulates a client that opens SSE but forgets to send initialized.
#[tokio::test]
async fn test_tools_call_without_initialized_notification_returns_timeout() {
    // SAFETY: This test runs in isolation and controls the env var
    unsafe { std::env::set_var("AHMA_HANDSHAKE_TIMEOUT_SECS", "1") };

    let server = spawn_test_server()
        .await
        .expect("Failed to spawn test server");
    let client = Client::new();

    // Step 1: Initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": { "roots": {} },
            "clientInfo": { "name": "no_initialized_test", "version": "0.1" }
        }
    });

    let init_resp = client
        .post(format!("{}/mcp", server.base_url()))
        .header(CONTENT_TYPE, "application/json")
        .json(&init_req)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("initialize POST failed");

    let session_id = init_resp
        .headers()
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .expect("Should have session ID")
        .to_string();

    // Step 2: Open SSE stream (but don't send initialized)
    let _sse_resp = client
        .get(format!("{}/mcp", server.base_url()))
        .header(ACCEPT, "text/event-stream")
        .header(MCP_SESSION_ID_HEADER, &session_id)
        .send()
        .await
        .expect("SSE GET failed");

    // Intentionally skip: initialized notification, roots/list response

    // Wait for handshake timeout
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Try to call a tool
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "cargo",
            "arguments": { "subcommand": "locate-project" }
        }
    });

    let resp = client
        .post(format!("{}/mcp", server.base_url()))
        .header(CONTENT_TYPE, "application/json")
        .header(MCP_SESSION_ID_HEADER, &session_id)
        .json(&tool_call)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("tools/call POST failed");

    assert_eq!(resp.status().as_u16(), 504);

    let body: Value = resp.json().await.expect("Response should be JSON");
    let error = body.get("error").expect("Should have error");
    let msg = error
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or_default();

    // SSE should be connected, but MCP initialized should be false
    assert!(
        msg.contains("SSE connected: true") && msg.contains("MCP initialized: false"),
        "Error should show SSE connected but MCP not initialized. Got: {msg}"
    );
}

/// Test that a properly completed handshake allows tools/call to succeed.
/// This validates the full VS Code-style handshake flow works correctly.
#[tokio::test]
async fn test_proper_vscode_handshake_allows_tool_calls() {
    use common::McpTestClient;
    use tempfile::TempDir;

    let server = spawn_test_server()
        .await
        .expect("Failed to spawn test server");

    // Create a temp directory with Cargo.toml for the test
    let temp_root = TempDir::new().expect("Failed to create temp root");
    let root_path = temp_root.path().to_path_buf();
    std::fs::write(
        root_path.join("Cargo.toml"),
        r#"[package]
name = "test-vscode-handshake"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("Failed to create Cargo.toml");

    // Use the test client which implements proper handshake
    let mut client = McpTestClient::for_server(&server);

    // This performs the full handshake: initialize → SSE → initialized → roots/list response
    client
        .initialize_with_roots("vscode-handshake-test", std::slice::from_ref(&root_path))
        .await
        .expect("Handshake should complete successfully");

    // Check if cargo tool is available (may not be in CI environment)
    let tools = client.list_tools().await.unwrap_or_default();
    let has_cargo = tools.iter().any(|t| {
        t.get("name")
            .and_then(|n| n.as_str())
            .is_some_and(|n| n == "cargo")
    });

    if !has_cargo {
        eprintln!("⚠️  Skipping tool call assertion - cargo tool not available");
        // Still pass the test - we verified the handshake worked
        return;
    }

    // Now tools/call should work
    let result = client
        .call_tool("cargo", json!({ "subcommand": "locate-project" }))
        .await;

    assert!(
        result.success,
        "Tool call should succeed after proper handshake. Error: {:?}",
        result.error
    );

    // Verify output contains the root path
    let output = result.output.unwrap_or_default();
    assert!(
        output.contains(root_path.to_string_lossy().as_ref()),
        "Output should contain root path. Got: {output}"
    );
}

/// Test that tools/call before handshake completion returns 409 (not timeout).
/// This is the expected behavior during the handshake window.
#[tokio::test]
async fn test_tools_call_during_handshake_returns_conflict() {
    // Use a long timeout so we hit 409, not 504
    // SAFETY: This test runs in isolation and controls the env var
    unsafe { std::env::set_var("AHMA_HANDSHAKE_TIMEOUT_SECS", "60") };

    let server = spawn_test_server()
        .await
        .expect("Failed to spawn test server");
    let client = Client::new();

    // Initialize only
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": { "roots": {} },
            "clientInfo": { "name": "conflict_test", "version": "0.1" }
        }
    });

    let init_resp = client
        .post(format!("{}/mcp", server.base_url()))
        .header(CONTENT_TYPE, "application/json")
        .json(&init_req)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("initialize POST failed");

    let session_id = init_resp
        .headers()
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .expect("Should have session ID")
        .to_string();

    // Immediately try tools/call (before timeout expires)
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "cargo",
            "arguments": { "subcommand": "locate-project" }
        }
    });

    let resp = client
        .post(format!("{}/mcp", server.base_url()))
        .header(CONTENT_TYPE, "application/json")
        .header(MCP_SESSION_ID_HEADER, &session_id)
        .json(&tool_call)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("tools/call POST failed");

    // Should return 409 Conflict (sandbox initializing, not timed out yet)
    assert_eq!(
        resp.status().as_u16(),
        409,
        "Expected HTTP 409 for sandbox initializing"
    );

    let body: Value = resp.json().await.expect("Response should be JSON");
    let error = body.get("error").expect("Should have error");

    // Verify error code is -32001 (sandbox initializing)
    assert_eq!(
        error.get("code").and_then(|c| c.as_i64()),
        Some(-32001),
        "Expected error code -32001 for sandbox initializing"
    );
}
