use anyhow::Context;
use futures::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
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
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();

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

    // Check for CARGO_TARGET_DIR
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_dir.join("target"));

    target_dir.join("debug/ahma_mcp")
}

async fn start_http_bridge_async(
    port: u16,
    tools_dir: &std::path::Path,
    sandbox_scope: &std::path::Path,
) -> std::process::Child {
    let binary = get_ahma_mcp_binary();

    // NOTE: Intentionally do NOT pass `--sync` here. We want async operations so progress
    // notifications can be emitted when (and only when) the client provides a progressToken.
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
            "--log-to-stderr",
        ])
        .env_remove("AHMA_TEST_MODE")
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_EXECUTION_MODE")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUST_TEST_THREADS")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start HTTP server");

    // Wait for server health.
    let client = Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", port);
    for _ in 0..30 {
        sleep(Duration::from_millis(100)).await;
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            return child;
        }
    }

    // Kill the child process before panicking to avoid zombie process
    let mut child = child;
    let _ = child.kill();
    let _ = child.wait();
    panic!("Timed out waiting for HTTP bridge to become healthy");
}

async fn send_mcp_request(
    client: &Client,
    base_url: &str,
    request: &Value,
    session_id: Option<&str>,
) -> anyhow::Result<(Value, Option<String>)> {
    let url = format!("{}/mcp", base_url);
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json");
    if let Some(sid) = session_id {
        req = req.header("Mcp-Session-Id", sid);
    }
    let resp = req.json(request).send().await.context("POST /mcp failed")?;
    let new_session_id = resp
        .headers()
        .get("mcp-session-id")
        .or_else(|| resp.headers().get("Mcp-Session-Id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body: Value = resp.json().await.context("Failed to parse JSON response")?;
    Ok((body, new_session_id))
}

async fn open_sse_stream(client: &Client, base_url: &str, session_id: &str) -> reqwest::Response {
    let url = format!("{}/mcp", base_url);
    let resp = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Mcp-Session-Id", session_id)
        .send()
        .await
        .expect("Failed to open SSE stream");
    assert!(resp.status().is_success(), "SSE stream must be available");
    resp
}

/// Drive the roots/list handshake over SSE and then keep the SSE stream open.
/// Returns a channel receiver which gets every JSON event payload parsed from SSE.
async fn sse_events_task(
    client: Client,
    base_url: String,
    session_id: String,
    roots: Vec<PathBuf>,
) -> tokio::sync::mpsc::Receiver<Value> {
    let resp = open_sse_stream(&client, &base_url, &session_id).await;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    let (tx, rx) = tokio::sync::mpsc::channel::<Value>(256);

    tokio::spawn(async move {
        loop {
            let chunk = stream.next().await;
            let Some(chunk) = chunk else { break };
            let Ok(bytes) = chunk else { break };
            buffer.push_str(&String::from_utf8_lossy(&bytes));

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

                // If we get roots/list, respond.
                if value.get("method").and_then(|m| m.as_str()) == Some("roots/list") {
                    let id = value.get("id").cloned().expect("roots/list must have id");
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
                        "result": { "roots": roots_json }
                    });
                    // Best-effort; if it fails, tests will fail later due to sandbox init.
                    let _ =
                        send_mcp_request(&client, &base_url, &response, Some(&session_id)).await;
                }

                let _ = tx.send(value).await;
            }
        }
    });

    rx
}

#[tokio::test]
async fn test_http_no_progress_token_does_not_emit_progress_notifications() -> anyhow::Result<()> {
    let server_scope_dir = TempDir::new().context("Failed to create temp dir (server_scope)")?;
    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).context("Failed to create tools dir")?;

    // Expose sandboxed_shell tool (async by default).
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();
    std::fs::copy(
        workspace_dir.join(".ahma/sandboxed_shell.json"),
        tools_dir.join("sandboxed_shell.json"),
    )
    .context("Failed to copy sandboxed_shell tool config")?;

    let sandbox_scope = server_scope_dir.path().to_path_buf();
    let port = find_available_port();
    let mut server = start_http_bridge_async(port, &tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    // initialize
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

    // Start SSE + roots/list handshake
    let client_root_dir = TempDir::new().context("Failed to create temp dir (client_root)")?;
    let mut events_rx = sse_events_task(
        client.clone(),
        base_url.clone(),
        session_id.clone(),
        vec![client_root_dir.path().to_path_buf()],
    )
    .await;

    // initialized
    let initialized = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
    let _ = send_mcp_request(&client, &base_url, &initialized, Some(&session_id)).await;

    // Give roots/list a moment to complete and sandbox to lock.
    sleep(Duration::from_millis(200)).await;

    // tools/call WITHOUT _meta.progressToken
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "sandboxed_shell",
            "arguments": {
                "command": "sleep 0.2",
                "working_directory": client_root_dir.path().to_string_lossy()
            }
        }
    });
    let (tool_resp, _) = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
        .await
        .context("tools/call failed")?;
    assert!(tool_resp.get("error").is_none(), "tools/call must succeed");

    // Assert: no notifications/progress arrive within a short window.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(ev)) =
            tokio::time::timeout(Duration::from_millis(200), events_rx.recv()).await
            && ev.get("method").and_then(|m| m.as_str()) == Some("notifications/progress")
        {
            anyhow::bail!("unexpected notifications/progress without client progressToken: {ev}");
        }
    }

    server.kill().ok();
    Ok(())
}

#[tokio::test]
async fn test_http_progress_token_is_echoed_in_progress_notifications() -> anyhow::Result<()> {
    let server_scope_dir = TempDir::new().context("Failed to create temp dir (server_scope)")?;
    let tools_dir = server_scope_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).context("Failed to create tools dir")?;

    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace dir")
        .to_path_buf();
    std::fs::copy(
        workspace_dir.join(".ahma/sandboxed_shell.json"),
        tools_dir.join("sandboxed_shell.json"),
    )
    .context("Failed to copy sandboxed_shell tool config")?;

    let sandbox_scope = server_scope_dir.path().to_path_buf();
    let port = find_available_port();
    let mut server = start_http_bridge_async(port, &tools_dir, &sandbox_scope).await;
    let base_url = format!("http://127.0.0.1:{}", port);
    let client = Client::new();

    // initialize
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

    // Start SSE + roots/list handshake
    let client_root_dir = TempDir::new().context("Failed to create temp dir (client_root)")?;
    let mut events_rx = sse_events_task(
        client.clone(),
        base_url.clone(),
        session_id.clone(),
        vec![client_root_dir.path().to_path_buf()],
    )
    .await;

    // initialized
    let initialized = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
    let _ = send_mcp_request(&client, &base_url, &initialized, Some(&session_id)).await;

    sleep(Duration::from_millis(200)).await;

    let token = "tok_http_1";
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "_meta": { "progressToken": token },
            "name": "sandboxed_shell",
            "arguments": {
                "command": "sleep 0.2",
                "working_directory": client_root_dir.path().to_string_lossy()
            }
        }
    });
    let (tool_resp, _) = send_mcp_request(&client, &base_url, &tool_call, Some(&session_id))
        .await
        .context("tools/call failed")?;
    assert!(tool_resp.get("error").is_none(), "tools/call must succeed");

    // Expect at least one notifications/progress with matching token.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(ev)) =
            tokio::time::timeout(Duration::from_millis(500), events_rx.recv()).await
        {
            if ev.get("method").and_then(|m| m.as_str()) != Some("notifications/progress") {
                continue;
            }
            let got = ev
                .get("params")
                .and_then(|p| p.get("progressToken"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert_eq!(
                got, token,
                "progressToken must be echoed from request _meta"
            );
            server.kill().ok();
            return Ok(());
        }
    }

    server.kill().ok();
    anyhow::bail!("did not observe notifications/progress with token {token}");
}
