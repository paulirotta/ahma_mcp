//! Regression test: Cargo must not write outside sandbox scope.
//!
//! This duplicates a real-world macOS failure mode:
//! `failed to open: <repo>/target/debug/.cargo-lock (Operation not permitted)`
//!
//! The repro is: a workspace configures Cargo `target-dir` outside the workspace root via
//! `.cargo/config.toml`. Ahma must force Cargo back under the tool call's `working_directory`.
//!
//! This test runs the same scenario in both stdio and HTTP modes.

use ahma_core::test_utils::get_workspace_dir;
use ahma_core::test_utils::test_client::new_client_in_dir_real;
use anyhow::Context;
use futures::StreamExt;
use reqwest::Client as HttpClient;
use rmcp::model::CallToolRequestParam;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;

fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to any port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

fn get_ahma_mcp_binary() -> PathBuf {
    let workspace_dir = get_workspace_dir();

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

async fn start_http_server(
    port: u16,
    tools_dir: &Path,
    sandbox_scope: &Path,
) -> std::process::Child {
    let binary = get_ahma_mcp_binary();

    let workspace_dir = get_workspace_dir();
    let guidance_file = workspace_dir.join(".ahma").join("tool_guidance.json");

    let child = Command::new(&binary)
        .args([
            "--mode",
            "http",
            "--http-port",
            &port.to_string(),
            "--sync",
            "--tools-dir",
            &tools_dir.to_string_lossy(),
            "--guidance-file",
            &guidance_file.to_string_lossy(),
            "--sandbox-scope",
            &sandbox_scope.to_string_lossy(),
            "--log-to-stderr",
        ])
        .env_remove("AHMA_TEST_MODE")
        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_EXECUTION_MODE")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUST_TEST_THREADS")
        // Important for long-running/stress tests: do not pipe stdout/stderr unless
        // we actively drain them. Piped-but-undrained output will eventually fill the
        // OS pipe buffer and deadlock the child process, causing HTTP timeouts.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start HTTP server");

    let client = HttpClient::new();
    let health_url = format!("http://127.0.0.1:{}/health", port);

    for _ in 0..30 {
        sleep(Duration::from_millis(200)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            return child;
        }
    }

    panic!("HTTP server failed to start (health check timeout)");
}

async fn send_mcp_request(
    client: &HttpClient,
    base_url: &str,
    request: &Value,
    session_id: Option<&str>,
) -> anyhow::Result<(Value, Option<String>)> {
    let mut req = client
        .post(format!("{}/mcp", base_url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(request)
        .timeout(Duration::from_secs(30));

    if let Some(id) = session_id {
        req = req.header("Mcp-Session-Id", id);
    }

    let response = req.send().await.context("POST /mcp failed")?;

    // Capture session id from initialize response.
    let session_id = response
        .headers()
        .get("mcp-session-id")
        .or_else(|| response.headers().get("Mcp-Session-Id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let json_value = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}))
    };

    if !status.is_success() {
        anyhow::bail!("HTTP {}: {}", status, json_value);
    }

    Ok((json_value, session_id))
}

async fn answer_roots_list_over_http_sse(
    client: &HttpClient,
    base_url: &str,
    session_id: &str,
    roots: &[PathBuf],
) {
    // Use a dedicated client for the roots/list response POST.
    // The SSE GET holds an open connection; sharing a connection pool here can
    // occasionally stall the POST under load.
    let post_client = HttpClient::new();

    let sse = client
        .get(format!("{}/mcp", base_url))
        .header("Accept", "text/event-stream")
        .header("Mcp-Session-Id", session_id)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .expect("SSE GET /mcp failed");

    assert!(sse.status().is_success(), "SSE not successful");

    let mut stream = sse.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("SSE chunk read failed");
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(idx) = buffer.find("\n\n") {
            let frame = buffer[..idx].to_string();
            buffer = buffer[idx + 2..].to_string();

            let data_lines: Vec<&str> = frame
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();

            for data in data_lines {
                let msg: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if msg.get("method").and_then(|m| m.as_str()) != Some("roots/list") {
                    continue;
                }

                let roots_id = msg.get("id").cloned().unwrap_or(json!(1));
                let roots_payload: Vec<Value> = roots
                    .iter()
                    .map(|p| {
                        let uri = format!("file://{}", p.to_string_lossy());
                        json!({"uri": uri, "name": "temp"})
                    })
                    .collect();

                let response = json!({
                    "jsonrpc": "2.0",
                    "id": roots_id,
                    "result": {"roots": roots_payload}
                });

                let _ = post_client
                    .post(format!("{}/mcp", base_url))
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json")
                    .header("Mcp-Session-Id", session_id)
                    .json(&response)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
                    .expect("roots/list response POST failed");

                return;
            }
        }
    }

    panic!("SSE ended before roots/list was answered");
}

async fn create_temp_rust_crate(root: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(root.join("src"))
        .await
        .context("Failed to create src")?;

    tokio::fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "ahma_permission_regression"
version = "0.1.0"
edition = "2021"
"#,
    )
    .await
    .context("Failed to write Cargo.toml")?;

    tokio::fs::write(root.join("src/main.rs"), "fn main() {}\n")
        .await
        .context("Failed to write main.rs")?;

    tokio::fs::create_dir_all(root.join(".cargo"))
        .await
        .context("Failed to create .cargo dir")?;

    tokio::fs::write(
        root.join(".cargo/config.toml"),
        r#"[build]
target-dir = "../OUTSIDE_SESSION_TARGET"
"#,
    )
    .await
    .context("Failed to write .cargo/config.toml")?;

    Ok(())
}

fn assert_cargo_writes_scoped_to_working_directory(root: &Path) {
    let in_scope_target = root.join("target");
    assert!(
        in_scope_target.exists(),
        "expected in-scope target dir at {in_scope_target:?}"
    );

    let out_of_scope_target = root
        .parent()
        .expect("client root must have parent")
        .join("OUTSIDE_SESSION_TARGET");

    if out_of_scope_target.exists() {
        let entries: Vec<_> = std::fs::read_dir(&out_of_scope_target)
            .unwrap_or_else(|_| panic!("Failed to read {out_of_scope_target:?}"))
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            entries.is_empty(),
            "cargo must not write outside the working_directory; found out-of-scope artifacts in {out_of_scope_target:?}: {entries:?}"
        );
    }
}

async fn run_stdio_mode(client_root: &Path, tools_dir: &Path) -> anyhow::Result<()> {
    // Start stdio server with *real* sandbox enforcement.
    // IMPORTANT: Under Landlock, the subprocess CWD must be inside the sandbox scope.
    // If we set CWD to the repo workspace, the process cannot even stat its own CWD
    // after restrict_self() and will fail immediately.
    let client = new_client_in_dir_real(
        Some(tools_dir.to_str().unwrap()),
        &[
            "--mode",
            "stdio",
            "--sync",
            "--sandbox-scope",
            client_root.to_str().unwrap(),
            "--log-to-stderr",
        ],
        client_root, // CWD must be inside sandbox scope for Landlock
    )
    .await
    .context("Failed to start stdio client")?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "cargo check -q",
                "working_directory": client_root.to_string_lossy(),
                "execution_mode": "Synchronous"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(r) => {
            let text: String = r
                .content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !text.contains("Operation not permitted"),
                "cargo check output contained EPERM: {text}"
            );
        }
        Err(e) => {
            let msg = format!("{e:#}");
            assert!(
                !msg.contains("Operation not permitted") && !msg.contains(".cargo-lock"),
                "cargo check failed with sandbox error: {msg}"
            );
            return Err(e.into());
        }
    }

    assert_cargo_writes_scoped_to_working_directory(client_root);

    client.cancel().await.ok();
    Ok(())
}

async fn run_http_mode(client_root: &Path, tools_dir: &Path) -> anyhow::Result<()> {
    let server_scope_dir = TempDir::new().context("Failed to create temp dir (server_scope)")?;
    let sandbox_scope = server_scope_dir.path().to_path_buf();

    let port = find_available_port();
    let mut server = start_http_server(port, tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = HttpClient::new();

    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });

    let (_init_response, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .context("initialize failed")?;
    let session_id = session_id.expect("initialize must return mcp-session-id header");

    // Answer roots/list over SSE.
    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_root = client_root.to_path_buf();
    let sse_task = tokio::spawn(async move {
        answer_roots_list_over_http_sse(&sse_client, &sse_base_url, &sse_session_id, &[sse_root])
            .await;
    });

    let initialized_notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(
        &client,
        &base_url,
        &initialized_notification,
        Some(&session_id),
    )
    .await;

    sse_task.await.expect("roots/list SSE task panicked");

    // Give the server a moment to complete sandbox locking after receiving roots/list response.
    // Under high load (e.g., parallel nextest), there can be a race between the POST return
    // and the server's internal sandbox lock completion.
    sleep(Duration::from_millis(100)).await;

    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "sandboxed_shell",
            "arguments": {
                "command": "cargo check -q",
                "working_directory": client_root.to_string_lossy()
            }
        }
    });

    let (tool_response, _) = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
        .await
        .context("tools/call failed")?;

    assert!(
        tool_response.get("error").is_none(),
        "cargo check (via sandboxed_shell) must succeed; got: {tool_response:?}"
    );

    assert_cargo_writes_scoped_to_working_directory(client_root);

    server.kill().ok();
    Ok(())
}

#[tokio::test]
async fn test_cargo_target_dir_scoped_in_stdio_and_http() {
    // Check if cargo is available on the system first.
    if std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("⚠️  Skipping test - cargo not available on system");
        return;
    }

    let client_scope_dir = TempDir::new().expect("Failed to create temp dir (client_scope)");
    let client_root = client_scope_dir.path();
    create_temp_rust_crate(client_root)
        .await
        .expect("Failed to create temp Rust crate");

    // Minimal tools dir containing sandboxed_shell.
    let tools_temp = TempDir::new().expect("Failed to create temp dir (tools)");
    let tools_dir = tools_temp.path();
    std::fs::copy(
        get_workspace_dir().join(".ahma/tools/sandboxed_shell.json"),
        tools_dir.join("sandboxed_shell.json"),
    )
    .expect("Failed to copy sandboxed_shell tool config");

    // Run both transports. This is intentionally redundant for comprehensive regression coverage.
    run_stdio_mode(client_root, tools_dir)
        .await
        .expect("stdio mode regression failed");

    // Give the filesystem a brief moment between modes.
    sleep(Duration::from_millis(100)).await;

    run_http_mode(client_root, tools_dir)
        .await
        .expect("http mode regression failed");
}

#[tokio::test]
async fn test_http_cargo_lock_permission_stress_60s() -> anyhow::Result<()> {
    // Always-on ~60s stress test to confirm real-world stability.
    // Focuses on the HTTP mode because that's where the reported EPERM regression occurs.

    if std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("⚠️  Skipping stress test - cargo not available on system");
        return Ok(());
    }

    let client_scope_dir = TempDir::new().expect("Failed to create temp dir (client_scope)");
    let client_root = client_scope_dir.path();
    create_temp_rust_crate(client_root)
        .await
        .expect("Failed to create temp Rust crate");

    let tools_temp = TempDir::new().expect("Failed to create temp dir (tools)");
    let tools_dir = tools_temp.path();
    std::fs::copy(
        get_workspace_dir().join(".ahma/tools/sandboxed_shell.json"),
        tools_dir.join("sandboxed_shell.json"),
    )
    .expect("Failed to copy sandboxed_shell tool config");

    let server_scope_dir = TempDir::new().expect("Failed to create temp dir (server_scope)");
    let sandbox_scope = server_scope_dir.path().to_path_buf();

    let port = find_available_port();
    let mut server = start_http_server(port, tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = HttpClient::new();

    // Create a single session and complete the MCP handshake once.
    // Stress the steady-state tools/call path rather than repeatedly spawning new sessions.
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "stress-client", "version": "1.0.0"}
        }
    });

    let (_init_response, session_id) = send_mcp_request(&client, &base_url, &init_request, None)
        .await
        .context("initialize failed")?;
    let session_id = session_id.expect("initialize must return mcp-session-id header");

    // Start SSE listener first, then send initialized, then respond to roots/list.
    // Close SSE after answering roots/list to avoid backpressure during the stress loop.
    let (roots_answered_tx, roots_answered_rx) = tokio::sync::oneshot::channel::<()>();
    let sse_client = client.clone();
    let sse_base_url = base_url.clone();
    let sse_session_id = session_id.clone();
    let sse_root = client_root.to_path_buf();
    let sse_task = tokio::spawn(async move {
        // Answer roots/list and return (dropping the SSE connection).
        answer_roots_list_over_http_sse(&sse_client, &sse_base_url, &sse_session_id, &[sse_root])
            .await;
        let _ = roots_answered_tx.send(());
    });

    let initialized_notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = send_mcp_request(
        &client,
        &base_url,
        &initialized_notification,
        Some(&session_id),
    )
    .await;

    tokio::time::timeout(Duration::from_secs(30), roots_answered_rx)
        .await
        .context("timeout waiting for roots/list to be answered")?
        .map_err(|_| anyhow::anyhow!("roots/list answered channel closed"))?;

    sse_task
        .await
        .expect("roots/list SSE task panicked during stress");

    let start = Instant::now();
    let mut iterations = 0u32;

    while start.elapsed() < Duration::from_secs(60) {
        iterations += 1;

        let tool_call = json!({
            "jsonrpc": "2.0",
            "id": 2 + iterations,
            "method": "tools/call",
            "params": {
                "name": "sandboxed_shell",
                "arguments": {
                    "command": "cargo check -q",
                    "working_directory": client_root.to_string_lossy()
                }
            }
        });

        let (tool_response, _) =
            send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
                .await
                .with_context(|| format!("tools/call failed on iteration {iterations}"))?;

        if let Some(error) = tool_response.get("error") {
            let msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("<no message>");
            panic!("stress iteration {iterations} failed: {msg}. full response: {tool_response:?}");
        }

        assert_cargo_writes_scoped_to_working_directory(client_root);

        // Small pacing delay to reduce flakiness and allow session cleanup.
        sleep(Duration::from_millis(50)).await;
    }

    println!(
        "✅ HTTP cargo-lock permission stress completed: {} iterations in {:?}",
        iterations,
        start.elapsed()
    );

    server.kill().ok();

    Ok(())
}
