//! HTTP Bridge Integration Tests
//!
//! These tests verify end-to-end HTTP bridge functionality by:
//! 1. Starting the HTTP bridge with a real ahma_mcp subprocess
//! 2. Sending requests through the HTTP interface
//! 3. Verifying correct responses
//!
//! These tests reproduce the bug where calling a tool from a different project
//! (different working_directory) fails with "expect initialized request" error.
//!
//! NOTE: These tests spawn their own servers with specific sandbox configurations.
//! They use dynamic port allocation to avoid conflicts with other tests.
//! The shared test server singleton (port 5721) is NOT used here.

use ahma_http_bridge::session::DEFAULT_HANDSHAKE_TIMEOUT_SECS;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::net::TcpListener;
use std::os::unix::fs as unix_fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Find an available port for testing (uses dynamic port allocation)
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

    // Check for CARGO_TARGET_DIR
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_dir.join("target"));

    let binary_path = target_dir.join("debug/ahma_mcp");

    // Optimization: Skip manual build if binary already exists, especially in NEXTEST
    // where all binaries are pre-built to avoid cargo lock contention.
    if !binary_path.exists() {
        // Build ahma_mcp binary
        let output = Command::new("cargo")
            .current_dir(&workspace_dir)
            .args(["build", "--package", "ahma_mcp", "--bin", "ahma_mcp"])
            .output()
            .expect("Failed to run cargo build");

        assert!(
            output.status.success(),
            "Failed to build ahma_mcp: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    binary_path
}

/// Start the HTTP bridge server and return the process and URL
async fn start_http_bridge(
    port: u16,
    tools_dir: &std::path::Path,
    sandbox_scope: &std::path::Path,
) -> std::process::Child {
    let binary = get_ahma_mcp_binary();

    let child = Command::new(&binary)
        .args([
            "--mode",
            "http",
            "--http-port",
            &port.to_string(),
            "--sync",
            "--tools-dir",
            &tools_dir.to_string_lossy(),
            "--sandbox-scope",
            &sandbox_scope.to_string_lossy(),
            "--log-to-stderr",
        ])
        // IMPORTANT:
        // These integration tests are explicitly verifying real sandbox-scope behavior.
        // ahma_mcp auto-enables a permissive "test mode" (sandbox bypass + best-effort scope "/")
        // when certain env vars are present (e.g. NEXTEST, CARGO_TARGET_DIR, RUST_TEST_THREADS).
        // That makes tests pass even when real-life behavior fails.
        //
        // So we *clear* those env vars for the spawned server process to ensure it behaves
        // like a real user-launched server.
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_EXECUTION_MODE")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUST_TEST_THREADS")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start HTTP bridge");

    // Wait for server to be ready
    let client = Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", port);

    for _ in 0..150 {
        sleep(Duration::from_millis(200)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            return child;
        }
    }

    // Kill and wait for the child to prevent zombie process
    let mut child = child;
    let _ = child.kill();
    let _ = child.wait();

    // Capture stderr for debugging startup failures
    let stderr_output = if let Some(stderr) = child.stderr.take() {
        use std::io::Read;
        let mut buf = String::new();
        let _ = std::io::BufReader::new(stderr).read_to_string(&mut buf);
        buf
    } else {
        String::new()
    };

    panic!(
        "HTTP bridge failed to start within timeout. Stderr: {}",
        stderr_output
    );
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
        .timeout(Duration::from_secs(120));

    if let Some(id) = session_id {
        req = req.header("Mcp-Session-Id", id);
    }

    let response = req
        .json(request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {:?}", e))?;

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

/// Wait for a `roots/list` request over SSE and respond with the provided roots.
async fn answer_roots_list_over_sse(
    client: &Client,
    base_url: &str,
    session_id: &str,
    roots: &[PathBuf],
    wait_for_sandbox_configured: bool,
) {
    let url = format!("{}/mcp", base_url);
    let resp = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Mcp-Session-Id", session_id)
        .send()
        .await
        .expect("Failed to open SSE stream");

    assert!(
        resp.status().is_success(),
        "SSE stream must be available, got HTTP {}",
        resp.status()
    );

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    // Hard timeout: if session isolation is broken, we may never see roots/list.
    let roots_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let sandbox_deadline =
        tokio::time::Instant::now() + Duration::from_secs(DEFAULT_HANDSHAKE_TIMEOUT_SECS);
    let mut answered_roots = false;
    let mut saw_sandbox_configured = false;

    loop {
        if !answered_roots && tokio::time::Instant::now() > roots_deadline {
            panic!("Timed out waiting for roots/list over SSE (session isolation likely broken)");
        }
        if wait_for_sandbox_configured
            && answered_roots
            && tokio::time::Instant::now() > sandbox_deadline
        {
            panic!("Timed out waiting for sandbox/configured notification");
        }

        let chunk = tokio::time::timeout(Duration::from_millis(500), stream.next())
            .await
            .ok()
            .flatten();

        if let Some(next) = chunk {
            let bytes = next.expect("SSE stream read failed");
            let text = String::from_utf8_lossy(&bytes);
            buffer.push_str(&text);

            // SSE events are separated by a blank line.
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

                if method == Some("notifications/sandbox/failed") {
                    let error = value
                        .get("params")
                        .and_then(|p| p.get("error"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown");
                    panic!("Sandbox configuration failed: {}", error);
                }

                if method == Some("notifications/sandbox/configured") {
                    saw_sandbox_configured = true;
                    if wait_for_sandbox_configured && answered_roots {
                        return;
                    }
                    continue;
                }

                if method != Some("roots/list") {
                    continue;
                }

                let id = value
                    .get("id")
                    .cloned()
                    .expect("roots/list must include id");

                let roots_json: Vec<Value> = roots
                    .iter()
                    .map(|p| {
                        json!({
                            "uri": format!("file://{}", p.display()),
                            "name": p.file_name().and_then(|n| n.to_str()).unwrap_or("root")
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

                let _ = send_mcp_request(client, base_url, &response, Some(session_id))
                    .await
                    .expect("Failed to send roots/list response");
                answered_roots = true;

                if !wait_for_sandbox_configured {
                    return;
                }
                if saw_sandbox_configured {
                    return;
                }
            }
        }
    }
}

fn percent_encode_path_for_file_uri(path: &std::path::Path) -> String {
    // Percent-encode path characters commonly present in IDE roots (spaces/unicode).
    // We keep '/' and unreserved characters. This is sufficient for file:// URIs.
    let s = path.to_string_lossy();
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
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

/// Wait for a `roots/list` request over SSE and respond with provided URI strings.
async fn answer_roots_list_over_sse_with_uris(
    client: &Client,
    base_url: &str,
    session_id: &str,
    root_uris: &[String],
    wait_for_sandbox_configured: bool,
) {
    let url = format!("{}/mcp", base_url);
    let resp = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Mcp-Session-Id", session_id)
        .send()
        .await
        .expect("Failed to open SSE stream");

    assert!(
        resp.status().is_success(),
        "SSE stream must be available, got HTTP {}",
        resp.status()
    );

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    let roots_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let sandbox_deadline =
        tokio::time::Instant::now() + Duration::from_secs(DEFAULT_HANDSHAKE_TIMEOUT_SECS);
    let mut answered_roots = false;
    let mut saw_sandbox_configured = false;

    loop {
        if !answered_roots && tokio::time::Instant::now() > roots_deadline {
            panic!("Timed out waiting for roots/list over SSE (session isolation likely broken)");
        }
        if wait_for_sandbox_configured
            && answered_roots
            && tokio::time::Instant::now() > sandbox_deadline
        {
            panic!("Timed out waiting for sandbox/configured notification");
        }

        let chunk = tokio::time::timeout(Duration::from_millis(500), stream.next())
            .await
            .ok()
            .flatten();

        if let Some(next) = chunk {
            let bytes = next.expect("SSE stream read failed");
            let text = String::from_utf8_lossy(&bytes);
            buffer.push_str(&text);

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

                if method == Some("notifications/sandbox/failed") {
                    let error = value
                        .get("params")
                        .and_then(|p| p.get("error"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown");
                    panic!("Sandbox configuration failed: {}", error);
                }

                if method == Some("notifications/sandbox/configured") {
                    saw_sandbox_configured = true;
                    if wait_for_sandbox_configured && answered_roots {
                        return;
                    }
                    continue;
                }

                if method != Some("roots/list") {
                    continue;
                }

                let id = value
                    .get("id")
                    .cloned()
                    .expect("roots/list must include id");

                let roots_json: Vec<Value> = root_uris
                    .iter()
                    .map(|uri| {
                        json!({
                            "uri": uri,
                            "name": "root"
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

                let _ = send_mcp_request(client, base_url, &response, Some(session_id))
                    .await
                    .expect("Failed to send roots/list response");
                answered_roots = true;

                if !wait_for_sandbox_configured {
                    return;
                }
                if saw_sandbox_configured {
                    return;
                }
            }
        }
    }
}

/// REGRESSION TEST (DO NOT WEAKEN): Cross-repo working_directory must succeed.
///
/// Real-world failure this guards against:
/// - Start the HTTP server from repo A (e.g. `ahma_mcp` checkout).
/// - Connect from VS Code opened on repo B.
/// - VS Code sends `tools/call` with `working_directory` in repo B.
/// - If the server is incorrectly scoped to repo A, it fails with:
///   "Path '...' is outside the sandbox root '...'".
///
/// The correct behavior is **per-session sandbox isolation**:
/// the sandbox scope must be derived from the client's `roots/list` response,
/// so repo B is allowed for that session even if the server was started elsewhere.
///
/// WARNING TO FUTURE AI/MAINTAINERS:
/// - Do NOT change this test to accept either success OR sandbox failure.
/// - Do NOT enable AHMA_TEST_MODE for this test.
/// - Fix scoping/session isolation if this fails.
#[tokio::test]
async fn test_tool_call_with_different_working_directory() {
    // Create temp directories:
    // - server_scope: where the HTTP server is started (repo A)
    // - client_scope: simulated VS Code workspace root (repo B)
    let server_scope_dir = TempDir::new().expect("Failed to create temp dir (server_scope)");
    let client_scope_dir = TempDir::new().expect("Failed to create temp dir (client_scope)");

    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Create a simple tool config that proves working_directory is honored.
    // We use `pwd` so we can assert the output contains the requested working directory.
    let tool_config = json!({
        "name": "pwd",
        "description": "Print current working directory",
        "command": "pwd",
        "enabled": true,
        "subcommand": [{
            "name": "default",
            "description": "Print working directory"
        }]
    });
    std::fs::write(
        tools_dir.join("echo.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    // Server sandbox scope (what used to incorrectly apply to all clients)
    let sandbox_scope = server_scope_dir.path().to_path_buf();

    // Client workspace scope (what must apply to THIS session after roots/list)
    let different_project_path = client_scope_dir.path().to_path_buf();

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
            "capabilities": {
                "roots": { "listChanged": true }
            },
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

    let session_id = session_id.expect("Session isolation must return mcp-session-id header");

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

    // Open SSE and answer roots/list with the client workspace root.
    // This is what binds the per-session sandbox scope.
    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_root = different_project_path.clone();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_sse(
            &sse_client,
            &sse_base_url,
            &sse_session_id,
            &[sse_root],
            true,
        )
        .await;
    });

    send_mcp_request(
        &client,
        &base_url,
        &initialized_notification,
        Some(&session_id),
    )
    .await
    .expect("notifications/initialized should succeed");
    // Notifications don't return responses, that's OK

    // Ensure roots/list was observed and answered.
    sse_task.await.expect("roots/list SSE task panicked");

    // Step 3: Call a tool with working_directory OUTSIDE the server sandbox scope.
    // This MUST succeed because the session sandbox scope is derived from client roots.
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "pwd",
            "arguments": {
                "subcommand": "default",
                "execution_mode": "Synchronous",
                "working_directory": different_project_path.to_string_lossy()
            }
        }
    });

    // Retry loop for tools/call to handle race condition where sandbox initialization
    // (which happens in a background task after roots/list response) hasn't finished yet.
    let start = tokio::time::Instant::now();
    let mut tool_response;
    loop {
        if start.elapsed() > Duration::from_secs(60) {
            panic!("Timed out waiting for sandbox initialization");
        }

        let (resp, _) = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
            .await
            .expect("Tool call should not fail with connection error");

        tool_response = resp;

        if let Some(error) = tool_response.get("error")
            && let Some(msg) = error.get("message").and_then(|m| m.as_str())
            && msg.contains("Sandbox initializing from client roots")
        {
            sleep(Duration::from_millis(100)).await;
            continue;
        }

        break;
    }

    assert!(
        tool_response.get("error").is_none(),
        "Cross-repo tool call must succeed; got error: {:?}",
        tool_response
    );
    assert!(
        tool_response.get("result").is_some(),
        "Cross-repo tool call must return result; got: {:?}",
        tool_response
    );

    // Prove the working_directory was actually used.
    let output_text = tool_response
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let different_project_str = different_project_path.to_string_lossy();
    assert!(
        output_text.contains(different_project_str.as_ref()),
        "pwd output must include the requested working_directory. Output: {:?}",
        tool_response
    );

    // Clean up
    server.kill().expect("Failed to kill server");
}

// NOTE: test_cargo_target_dir_is_scoped_to_working_directory was removed.
// It tested cargo-specific CARGO_TARGET_DIR env var overrides that are no longer used.
// The sandbox now relies on OS-level restrictions (sandbox-exec on macOS, Landlock on Linux)
// which are generic and apply to all tools, not just cargo.

/// Test: Basic tool call within sandbox scope works correctly
#[tokio::test]
async fn test_basic_tool_call_within_sandbox() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Create pwd tool config
    let tool_config = json!({
        "name": "pwd",
        "description": "Print current working directory",
        "command": "pwd",
        "enabled": true,
        "subcommand": [{
            "name": "default",
            "description": "Print working directory"
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
            "capabilities": {
                "roots": { "listChanged": true }
            },
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

    let session_id_for_requests =
        session_id.expect("Session isolation must return mcp-session-id header");

    // Send initialized notification
    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    // Open SSE and answer roots/list with the sandbox scope.
    // In always-on session isolation, the subprocess runs with --defer-sandbox and
    // tool execution is blocked until roots/list has been answered.
    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id_for_requests.clone();
    let sse_root = sandbox_scope.clone();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_sse(
            &sse_client,
            &sse_base_url,
            &sse_session_id,
            &[sse_root],
            true,
        )
        .await;
    });

    let _ = send_mcp_request(
        &client,
        &base_url,
        &initialized,
        Some(&session_id_for_requests),
    )
    .await;

    // Ensure roots/list was observed and answered.
    sse_task.await.expect("roots/list SSE task panicked");

    // Call tool WITHIN sandbox scope
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "pwd",
            "arguments": {
                "subcommand": "default",
                "working_directory": sandbox_scope.to_string_lossy()
            }
        }
    });

    // Retry loop for tools/call to handle race condition where sandbox initialization
    // (which happens in a background task after roots/list response) hasn't finished yet.
    let start = tokio::time::Instant::now();
    let mut response;
    loop {
        if start.elapsed() > Duration::from_secs(60) {
            panic!("Timed out waiting for sandbox initialization");
        }

        eprintln!(
            "RETRY LOOP: Sending tools/call (elapsed: {:?})",
            start.elapsed()
        );
        let (resp, _) = send_mcp_request(
            &client,
            &base_url,
            &tool_call,
            Some(&session_id_for_requests),
        )
        .await
        .expect("Tool call should succeed");

        response = resp;

        if let Some(error) = response.get("error")
            && let Some(msg) = error.get("message").and_then(|m| m.as_str())
            && msg.contains("Sandbox initializing from client roots")
        {
            sleep(Duration::from_millis(100)).await;
            continue;
        }

        break;
    }

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

/// Roots URIs may be percent-encoded (spaces/unicode) by real IDE clients.
/// Session isolation must decode these correctly so sandbox scope matches the workspace.
#[tokio::test]
async fn test_roots_uri_parsing_percent_encoded_path() {
    let server_scope_dir = TempDir::new().expect("Failed to create temp dir (server_scope)");
    let client_scope_dir = TempDir::new().expect("Failed to create temp dir (client_scope)");

    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Create pwd tool config
    let tool_config = json!({
        "name": "pwd",
        "description": "Print current working directory",
        "command": "pwd",
        "enabled": true,
        "subcommand": [{
            "name": "default",
            "description": "Print working directory"
        }]
    });
    std::fs::write(
        tools_dir.join("pwd.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    // Make a workspace root with space + unicode in the path.
    let client_root = client_scope_dir.path().join("my proj ✓");
    tokio::fs::create_dir_all(&client_root)
        .await
        .expect("Failed to create client root");

    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, server_scope_dir.path()).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true }
            },
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (_, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .expect("Initialize should succeed");
    let session_id = session_id.expect("Session isolation must return mcp-session-id header");

    let encoded_path = percent_encode_path_for_file_uri(&client_root);
    let uri = format!("file://{}", encoded_path);

    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_uri = uri.clone();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_sse_with_uris(
            &sse_client,
            &sse_base_url,
            &sse_session_id,
            &[sse_uri],
            true,
        )
        .await;
    });

    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(&client, &base_url, &initialized, Some(&session_id)).await;
    sse_task.await.expect("roots/list SSE task panicked");

    // Retry loop for tool call - sandbox lock may take a moment after roots/list response
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "pwd",
            "arguments": {
                "subcommand": "default",
                "working_directory": client_root.to_string_lossy()
            }
        }
    });

    // Retry loop with exponential backoff - handles both "Sandbox initializing" errors
    // AND transport timeouts that can occur on slow CI runners (especially llvm-cov)
    let mut resp = None;
    let max_attempts = 20; // Increased for CI tolerance
    for attempt in 0..max_attempts {
        let result = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id)).await;

        match result {
            Ok((r, _)) => {
                // Check if we got a "sandbox initializing" retry error
                if let Some(err) = r.get("error") {
                    let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    if msg.contains("Sandbox initializing") {
                        eprintln!(
                            "Retry {}/{}: Sandbox still initializing",
                            attempt + 1,
                            max_attempts
                        );
                        sleep(Duration::from_millis(100 * (attempt + 1) as u64)).await;
                        continue;
                    }
                }
                resp = Some(r);
                break;
            }
            Err(e) if e.contains("timeout") || e.contains("500") => {
                // CI may be slow due to coverage instrumentation or resource contention
                eprintln!(
                    "Retry {}/{}: Transport error: {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                sleep(Duration::from_millis(200 * (attempt + 1) as u64)).await;
                continue;
            }
            Err(e) => {
                panic!("Unexpected error during tool call: {}", e);
            }
        }
    }
    let resp = resp.unwrap_or_else(|| {
        panic!(
            "Tool call should eventually succeed after {} attempts",
            max_attempts
        )
    });

    assert!(
        resp.get("error").is_none(),
        "pwd must succeed, got: {resp:?}"
    );

    let output_text = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(
        output_text.contains(client_root.to_string_lossy().as_ref()),
        "pwd output must include decoded client root path; got: {resp:?}"
    );

    server.kill().expect("Failed to kill server");
}

/// Some clients send file URIs in host form: file://localhost/abs/path
#[tokio::test]
async fn test_roots_uri_parsing_file_localhost() {
    let server_scope_dir = TempDir::new().expect("Failed to create temp dir (server_scope)");
    let client_scope_dir = TempDir::new().expect("Failed to create temp dir (client_scope)");

    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    let tool_config = json!({
        "name": "pwd",
        "description": "Print current working directory",
        "command": "pwd",
        "enabled": true,
        "subcommand": [{
            "name": "default",
            "description": "Print working directory"
        }]
    });
    std::fs::write(
        tools_dir.join("pwd.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    let client_root = client_scope_dir.path().join("my proj ✓");
    tokio::fs::create_dir_all(&client_root)
        .await
        .expect("Failed to create client root");

    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, server_scope_dir.path()).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true }
            },
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (_, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .expect("Initialize should succeed");
    let session_id = session_id.expect("Session isolation must return mcp-session-id header");

    let encoded_path = percent_encode_path_for_file_uri(&client_root);
    let uri = format!("file://localhost{}", encoded_path);

    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_uri = uri.clone();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_sse_with_uris(
            &sse_client,
            &sse_base_url,
            &sse_session_id,
            &[sse_uri],
            true,
        )
        .await;
    });

    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(&client, &base_url, &initialized, Some(&session_id)).await;
    sse_task.await.expect("roots/list SSE task panicked");

    // Retry loop for tool call - sandbox lock may take a moment after roots/list response
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "pwd",
            "arguments": {
                "subcommand": "default",
                "working_directory": client_root.to_string_lossy()
            }
        }
    });

    // Retry loop with exponential backoff - handles both "Sandbox initializing" errors
    // AND transport timeouts that can occur on slow CI runners (especially llvm-cov)
    let mut resp = None;
    let max_attempts = 20; // Increased for CI tolerance  
    for attempt in 0..max_attempts {
        let result = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id)).await;

        match result {
            Ok((r, _)) => {
                // Check if we got a "sandbox initializing" retry error
                if let Some(err) = r.get("error") {
                    let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    if msg.contains("Sandbox initializing") {
                        eprintln!(
                            "Retry {}/{}: Sandbox still initializing",
                            attempt + 1,
                            max_attempts
                        );
                        sleep(Duration::from_millis(100 * (attempt + 1) as u64)).await;
                        continue;
                    }
                }
                resp = Some(r);
                break;
            }
            Err(e) if e.contains("timeout") || e.contains("500") => {
                // CI may be slow due to coverage instrumentation or resource contention
                eprintln!(
                    "Retry {}/{}: Transport error: {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                sleep(Duration::from_millis(200 * (attempt + 1) as u64)).await;
                continue;
            }
            Err(e) => {
                panic!("Unexpected error during tool call: {}", e);
            }
        }
    }
    let resp = resp.unwrap_or_else(|| {
        panic!(
            "Tool call should eventually succeed after {} attempts",
            max_attempts
        )
    });

    assert!(
        resp.get("error").is_none(),
        "pwd must succeed, got: {resp:?}"
    );

    server.kill().expect("Failed to kill server");
}

/// Red-team: working_directory with '..' that resolves outside root must be rejected.
#[tokio::test]
async fn test_rejects_working_directory_path_traversal_outside_root() {
    let server_scope_dir = TempDir::new().expect("Failed to create temp dir (server_scope)");
    let sandbox_parent = TempDir::new().expect("Failed to create temp dir (sandbox_parent)");

    let client_root = sandbox_parent.path().join("root");
    let outside_dir = sandbox_parent.path().join("outside");
    tokio::fs::create_dir_all(&client_root)
        .await
        .expect("Failed to create client root");
    tokio::fs::create_dir_all(&outside_dir)
        .await
        .expect("Failed to create outside dir");

    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    let tool_config = json!({
        "name": "pwd",
        "description": "Print current working directory",
        "command": "pwd",
        "enabled": true,
        "subcommand": [{
            "name": "default",
            "description": "Print working directory"
        }]
    });
    std::fs::write(
        tools_dir.join("pwd.json"),
        serde_json::to_string_pretty(&tool_config).unwrap(),
    )
    .expect("Failed to write tool config");

    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, server_scope_dir.path()).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true }
            },
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (_, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .expect("Initialize should succeed");
    let session_id = session_id.expect("Session isolation must return mcp-session-id header");

    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_root = client_root.clone();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_sse(
            &sse_client,
            &sse_base_url,
            &sse_session_id,
            &[sse_root],
            true,
        )
        .await;
    });

    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(&client, &base_url, &initialized, Some(&session_id)).await;
    sse_task.await.expect("roots/list SSE task panicked");

    let traversal = client_root
        .join("subdir")
        .join("..")
        .join("..")
        .join("outside");

    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "pwd",
            "arguments": {
                "subcommand": "default",
                "working_directory": traversal.to_string_lossy()
            }
        }
    });

    // Retry loop for tools/call to handle race condition where sandbox initialization
    // (which happens in a background task after roots/list response) hasn't finished yet.
    let start = tokio::time::Instant::now();
    let mut resp;
    loop {
        if start.elapsed() > Duration::from_secs(60) {
            let _ = server.kill();
            let output = server.wait_with_output().expect("Failed to wait on server");
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!(
                "Timed out waiting for sandbox initialization. Server logs:\n{}",
                stderr
            );
        }

        let (r, _) = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
            .await
            .expect("Request should succeed at transport layer");

        resp = r;

        if let Some(error) = resp.get("error")
            && let Some(msg) = error.get("message").and_then(|m| m.as_str())
            && msg.contains("Sandbox initializing from client roots")
        {
            sleep(Duration::from_millis(100)).await;
            continue;
        }
        break;
    }

    assert!(
        resp.get("error").is_some(),
        "Expected sandbox rejection, got: {resp:?}"
    );
    let msg = resp
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    assert!(
        msg.contains("outside") && msg.contains("sandbox"),
        "Expected sandbox boundary error message, got: {msg:?}"
    );

    server.kill().expect("Failed to kill server");
}

/// Red-team: symlink inside root pointing outside must not allow writes outside root.
#[tokio::test]
async fn test_symlink_escape_attempt_is_blocked() {
    let server_scope_dir = TempDir::new().expect("Failed to create temp dir (server_scope)");
    let sandbox_parent = TempDir::new().expect("Failed to create temp dir (sandbox_parent)");

    let client_root = sandbox_parent.path().join("root");
    let outside_dir = sandbox_parent.path().join("outside");
    tokio::fs::create_dir_all(&client_root)
        .await
        .expect("Failed to create client root");
    tokio::fs::create_dir_all(&outside_dir)
        .await
        .expect("Failed to create outside dir");

    // Symlink inside root -> outside
    let escape_link = client_root.join("escape");
    unix_fs::symlink(&outside_dir, &escape_link).expect("Failed to create symlink");

    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).expect("Failed to create tools dir");

    // Copy the workspace file_tools config so we can attempt a write.
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();
    std::fs::copy(
        workspace_dir.join("ahma_mcp/examples/configs/file_tools.json"),
        tools_dir.join("file_tools.json"),
    )
    .expect("Failed to copy file_tools tool config");

    let port = find_available_port();
    let mut server = start_http_bridge(port, &tools_dir, server_scope_dir.path()).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true }
            },
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let (_, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .expect("Initialize should succeed");
    let session_id = session_id.expect("Session isolation must return mcp-session-id header");

    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_root = client_root.clone();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_sse(
            &sse_client,
            &sse_base_url,
            &sse_session_id,
            &[sse_root],
            true,
        )
        .await;
    });

    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(&client, &base_url, &initialized, Some(&session_id)).await;
    sse_task.await.expect("roots/list SSE task panicked");

    // Attempt to create a file that would resolve outside the sandbox via symlink.
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "file_tools",
            "arguments": {
                "subcommand": "touch",
                "working_directory": client_root.to_string_lossy(),
                "files": ["escape/owned.txt"]
            }
        }
    });

    // Retry loop for tools/call to handle race condition where sandbox initialization
    // (which happens in a background task after roots/list response) hasn't finished yet.
    let start = tokio::time::Instant::now();
    let mut resp;
    loop {
        if start.elapsed() > Duration::from_secs(60) {
            let _ = server.kill();
            let output = server.wait_with_output().expect("Failed to wait on server");
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!(
                "Timed out waiting for sandbox initialization. Server logs:\n{}",
                stderr
            );
        }

        let (r, _) = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
            .await
            .expect("Request should succeed at transport layer");

        resp = r;

        if let Some(error) = resp.get("error")
            && let Some(msg) = error.get("message").and_then(|m| m.as_str())
            && msg.contains("Sandbox initializing from client roots")
        {
            sleep(Duration::from_millis(100)).await;
            continue;
        }
        break;
    }

    assert!(
        !outside_dir.join("owned.txt").exists(),
        "Symlink escape must not create files outside sandbox root"
    );

    // file_tools failures may be represented either as a JSON-RPC error or as result.isError=true.
    let is_jsonrpc_error = resp.get("error").is_some();
    let is_tool_error = resp
        .get("result")
        .and_then(|r| r.get("isError"))
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    assert!(
        is_jsonrpc_error || is_tool_error,
        "Expected sandbox/tool rejection signal, got: {resp:?}"
    );

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

    // Create pwd tool config
    let tool_config = json!({
        "name": "pwd",
        "description": "Print current working directory",
        "command": "pwd",
        "enabled": true,
        "subcommand": [{
            "name": "default",
            "description": "Print working directory"
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
            "name": "pwd",
            "arguments": {
                "subcommand": "default",
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
