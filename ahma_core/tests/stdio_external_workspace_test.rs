//! Stdio External Workspace Integration Test
//!
//! This test reproduces the real-world scenario where:
//! 1. The ahma_mcp binary is invoked from a project OUTSIDE the ahma_mcp workspace
//! 2. The sandbox scope is derived from the client's roots/list response
//! 3. AHMA_TEST_MODE is NOT set (real sandbox validation)
//!
//! This is designed to catch the "command treated as filename" bug reported when
//! using Ahma MCP via stdio from Cursor in another Rust project.

use anyhow::Context;
use rmcp::{
    ServiceExt,
    model::{
        CallToolRequest, CallToolRequestParams, ClientCapabilities, ClientRequest, ClientResult,
        Implementation, InitializeRequestParams, ListRootsResult, ProtocolVersion, Root,
        ServerNotification, ServerRequest, ServerResult,
    },
    service::{NotificationContext, RequestContext, RoleClient},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::sync::mpsc;

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf()
}

/// Creates a tools directory with a minimal sandboxed_shell config
fn create_tools_dir(temp: &TempDir) -> anyhow::Result<PathBuf> {
    let tools_dir = temp.path().join("tools");
    std::fs::create_dir_all(&tools_dir)?;
    std::fs::write(
        tools_dir.join("sandboxed_shell.json"),
        r#"{
  "name": "sandboxed_shell",
  "description": "Execute shell commands within a secure sandbox.",
  "command": "bash -c",
  "enabled": true,
  "timeout_seconds": 30,
  "synchronous": true,
  "subcommand": [
    {
      "name": "default",
      "description": "Execute a shell command",
      "positional_args": [
        { "name": "command", "type": "string", "required": true }
      ],
      "options": [
        { "name": "working_directory", "type": "string", "format": "path" }
      ]
    }
  ]
}"#,
    )?;
    Ok(tools_dir)
}

/// Client that responds to roots/list requests with the specified roots
#[derive(Clone)]
struct ExternalWorkspaceClient {
    roots: Vec<PathBuf>,
    roots_requested: Arc<AtomicBool>,
    error_tx: mpsc::Sender<String>,
}

#[allow(clippy::manual_async_fn)]
impl rmcp::service::Service<RoleClient> for ExternalWorkspaceClient {
    fn get_info(&self) -> InitializeRequestParams {
        InitializeRequestParams {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "external-workspace-test-client".into(),
                title: None,
                version: "1.0.0".into(),
                icons: None,
                website_url: None,
            },
            meta: None,
        }
    }

    fn handle_request(
        &self,
        request: ServerRequest,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<ClientResult, rmcp::model::ErrorData>> + Send + '_
    {
        async move {
            match request {
                ServerRequest::ListRootsRequest(_req) => {
                    eprintln!("[TestClient] Received roots/list request");
                    self.roots_requested.store(true, Ordering::SeqCst);

                    let roots: Vec<Root> = self
                        .roots
                        .iter()
                        .map(|p| Root {
                            uri: format!("file://{}", p.display()),
                            name: p
                                .file_name()
                                .and_then(|n| n.to_str())
                                .map(|s| s.to_string()),
                        })
                        .collect();
                    eprintln!("[TestClient] Returning {} roots: {:?}", roots.len(), roots);
                    Ok(ClientResult::ListRootsResult(ListRootsResult { roots }))
                }
                _ => Ok(ClientResult::empty(())),
            }
        }
    }

    fn handle_notification(
        &self,
        notification: ServerNotification,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), rmcp::model::ErrorData>> + Send + '_ {
        let error_tx = self.error_tx.clone();
        async move {
            // Log any notifications for debugging
            match &notification {
                ServerNotification::ProgressNotification(n) => {
                    eprintln!("[TestClient] Progress: {:?}", n.params);
                }
                ServerNotification::LoggingMessageNotification(log) => {
                    if log.params.level == rmcp::model::LoggingLevel::Error {
                        let _ = error_tx
                            .send(format!("Server error: {:?}", log.params.data))
                            .await;
                    }
                }
                _ => {
                    eprintln!("[TestClient] Received notification: {:?}", notification);
                }
            }
            Ok(())
        }
    }
}

/// REGRESSION TEST: sandboxed_shell must work when invoked from external workspace via stdio
///
/// This test reproduces the real-world scenario where:
/// 1. A user has Cursor open on project B (NOT the ahma_mcp workspace)
/// 2. Cursor launches ahma_mcp via stdio
/// 3. The client provides project B as the root via roots/list
/// 4. sandboxed_shell is called with a command like "echo hello"
///
/// The bug: "treats entire command strings as filenames instead of parsing them correctly"
///
/// This test does NOT use AHMA_TEST_MODE to ensure real sandbox validation occurs.
#[tokio::test]
async fn test_sandboxed_shell_from_external_workspace_stdio() -> anyhow::Result<()> {
    // Create a temp directory that simulates an "external project" workspace
    // This is OUTSIDE the ahma_mcp workspace
    let external_workspace =
        TempDir::new().context("Failed to create temp dir for external workspace")?;
    let external_workspace_path = external_workspace.path().to_path_buf();

    // Create tools dir inside the external workspace
    let tools_dir =
        create_tools_dir(&external_workspace).context("Failed to create tools dir")?;

    let (error_tx, mut error_rx) = mpsc::channel::<String>(128);

    let client_impl = ExternalWorkspaceClient {
        roots: vec![external_workspace_path.clone()],
        roots_requested: Arc::new(AtomicBool::new(false)),
        error_tx,
    };
    let roots_requested = client_impl.roots_requested.clone();

    // Get the ahma_mcp binary path
    let wd = workspace_dir();
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| wd.join("target"));
    let binary_path = target_dir.join("debug/ahma_mcp");

    eprintln!(
        "[Test] Starting ahma_mcp from external workspace: {:?}",
        external_workspace_path
    );
    eprintln!("[Test] Binary path: {:?}", binary_path);
    eprintln!("[Test] Tools dir: {:?}", tools_dir);

    // Spawn ahma_mcp with current_dir set to the EXTERNAL workspace (not ahma_mcp workspace)
    // This simulates the real scenario where the binary runs from a different project
    let service = client_impl
        .clone()
        .serve(TokioChildProcess::new(
            Command::new(&binary_path).configure(|cmd| {
                cmd.arg("--tools-dir")
                    .arg(&tools_dir)
                    .arg("--log-to-stderr")
                    // CRITICAL: Set current_dir to EXTERNAL workspace, not the ahma_mcp workspace
                    .current_dir(&external_workspace_path)
                    // CRITICAL: Do NOT set AHMA_TEST_MODE - we want real sandbox validation
                    // Remove test mode env vars that might be inherited from test runner
                    .env_remove("AHMA_TEST_MODE")
                    .env_remove("NEXTEST")
                    .env_remove("NEXTEST_EXECUTION_MODE")
                    .env_remove("CARGO_TARGET_DIR")
                    .env_remove("RUST_TEST_THREADS");
            }),
        )?)
        .await
        .context("Failed to start rmcp client + ahma_mcp stdio")?;

    // Wait for initialization and roots/list handshake
    // The server should request roots/list after initialization
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if roots_requested.load(Ordering::SeqCst) {
            eprintln!("[Test] roots/list was requested");
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if !roots_requested.load(Ordering::SeqCst) {
        service.cancel().await.ok();
        anyhow::bail!("roots/list was never requested - MCP handshake may have failed");
    }

    // Give the server a moment to update sandbox scopes after receiving roots
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Now call sandboxed_shell with a simple command
    // This is where the bug manifests: the command "echo hello" might be
    // treated as a filename instead of a shell command
    let params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "echo hello",
                "working_directory": external_workspace_path.to_string_lossy(),
                "execution_mode": "Synchronous"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    eprintln!("[Test] Calling sandboxed_shell with command='echo hello'");

    let req = ClientRequest::CallToolRequest(CallToolRequest::new(params));
    let response = service
        .send_request_with_option(
            req,
            rmcp::service::PeerRequestOptions {
                timeout: Some(Duration::from_secs(10)),
                meta: None,
            },
        )
        .await
        .context("Failed to send tool call request")?
        .await_response()
        .await
        .context("Failed to receive tool call response")?;

    eprintln!("[Test] Response: {:?}", response);

    // Check for any server errors logged
    if let Ok(Some(error)) =
        tokio::time::timeout(Duration::from_millis(100), error_rx.recv()).await
    {
        eprintln!("[Test] Server logged error: {}", error);
    }

    // Clean up
    service.cancel().await.ok();

    // Verify the response - response is ServerResult for client calling server
    match response {
        ServerResult::CallToolResult(result) => {
            // Check if there was an error
            if result.is_error == Some(true) {
                let error_text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");

                // This is where we expect to catch the bug
                if error_text.contains("outside the sandbox")
                    || error_text.contains("filename")
                    || error_text.contains("No such file")
                    || error_text.contains("cannot find")
                {
                    anyhow::bail!(
                        "sandboxed_shell treated command as filename or path: {}",
                        error_text
                    );
                }

                anyhow::bail!("sandboxed_shell returned error: {}", error_text);
            }

            // Success case - verify output contains "hello"
            let output_text = result
                .content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                .collect::<Vec<_>>()
                .join("\n");

            eprintln!("[Test] Tool output: {}", output_text);

            assert!(
                output_text.contains("hello") || output_text.contains("Synchronous"),
                "sandboxed_shell output should contain 'hello' or be async response. Got: {}",
                output_text
            );

            Ok(())
        }
        other => {
            anyhow::bail!(
                "Unexpected response type from tools/call: {:?}",
                other
            );
        }
    }
}

/// Test sandboxed_shell using the built-in tool (no config file) from external workspace
/// This tests the built-in sandboxed_shell implementation in mcp_service
#[tokio::test]
async fn test_builtin_sandboxed_shell_from_external_workspace_stdio() -> anyhow::Result<()> {
    // Create a temp directory that simulates an "external project" workspace
    let external_workspace =
        TempDir::new().context("Failed to create temp dir for external workspace")?;
    let external_workspace_path = external_workspace.path().to_path_buf();

    // Create an EMPTY tools directory - no sandboxed_shell.json config
    // This forces the use of the built-in sandboxed_shell tool
    let tools_dir = external_workspace_path.join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    let (error_tx, _error_rx) = mpsc::channel::<String>(128);

    let client_impl = ExternalWorkspaceClient {
        roots: vec![external_workspace_path.clone()],
        roots_requested: Arc::new(AtomicBool::new(false)),
        error_tx,
    };
    let roots_requested = client_impl.roots_requested.clone();

    let wd = workspace_dir();
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| wd.join("target"));
    let binary_path = target_dir.join("debug/ahma_mcp");

    eprintln!(
        "[Test] Starting ahma_mcp with built-in sandboxed_shell from: {:?}",
        external_workspace_path
    );

    let service = client_impl
        .clone()
        .serve(TokioChildProcess::new(
            Command::new(&binary_path).configure(|cmd| {
                cmd.arg("--tools-dir")
                    .arg(&tools_dir)
                    .arg("--log-to-stderr")
                    .current_dir(&external_workspace_path)
                    .env_remove("AHMA_TEST_MODE")
                    .env_remove("NEXTEST")
                    .env_remove("NEXTEST_EXECUTION_MODE")
                    .env_remove("CARGO_TARGET_DIR")
                    .env_remove("RUST_TEST_THREADS");
            }),
        )?)
        .await
        .context("Failed to start rmcp client + ahma_mcp stdio")?;

    // Wait for roots/list handshake
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if roots_requested.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if !roots_requested.load(Ordering::SeqCst) {
        service.cancel().await.ok();
        anyhow::bail!("roots/list was never requested");
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Call the built-in sandboxed_shell with synchronous mode
    let params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "echo builtin_test",
                "working_directory": external_workspace_path.to_string_lossy(),
                "execution_mode": "Synchronous"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    eprintln!("[Test] Calling built-in sandboxed_shell with command='echo builtin_test'");

    let req = ClientRequest::CallToolRequest(CallToolRequest::new(params));
    let response = service
        .send_request_with_option(
            req,
            rmcp::service::PeerRequestOptions {
                timeout: Some(Duration::from_secs(10)),
                meta: None,
            },
        )
        .await?
        .await_response()
        .await?;

    service.cancel().await.ok();

    match response {
        ServerResult::CallToolResult(result) => {
            if result.is_error == Some(true) {
                let error_text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");

                if error_text.contains("outside the sandbox")
                    || error_text.contains("filename")
                    || error_text.contains("No such file")
                {
                    anyhow::bail!(
                        "Built-in sandboxed_shell treated command as filename: {}",
                        error_text
                    );
                }

                anyhow::bail!("Built-in sandboxed_shell returned error: {}", error_text);
            }

            let output_text = result
                .content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                .collect::<Vec<_>>()
                .join("\n");

            eprintln!("[Test] Built-in tool output: {}", output_text);

            assert!(
                output_text.contains("builtin_test"),
                "Built-in sandboxed_shell should output 'builtin_test'. Got: {}",
                output_text
            );

            Ok(())
        }
        other => {
            anyhow::bail!("Unexpected response type: {:?}", other);
        }
    }
}

/// Test that sandboxed_shell works with async execution mode from external workspace
#[tokio::test]
async fn test_sandboxed_shell_async_from_external_workspace_stdio() -> anyhow::Result<()> {
    let external_workspace =
        TempDir::new().context("Failed to create temp dir for external workspace")?;
    let external_workspace_path = external_workspace.path().to_path_buf();
    let tools_dir =
        create_tools_dir(&external_workspace).context("Failed to create tools dir")?;

    let (error_tx, _error_rx) = mpsc::channel::<String>(128);

    let client_impl = ExternalWorkspaceClient {
        roots: vec![external_workspace_path.clone()],
        roots_requested: Arc::new(AtomicBool::new(false)),
        error_tx,
    };
    let roots_requested = client_impl.roots_requested.clone();

    let wd = workspace_dir();
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| wd.join("target"));
    let binary_path = target_dir.join("debug/ahma_mcp");

    let service = client_impl
        .clone()
        .serve(TokioChildProcess::new(
            Command::new(&binary_path).configure(|cmd| {
                cmd.arg("--tools-dir")
                    .arg(&tools_dir)
                    .arg("--log-to-stderr")
                    .current_dir(&external_workspace_path)
                    .env_remove("AHMA_TEST_MODE")
                    .env_remove("NEXTEST")
                    .env_remove("NEXTEST_EXECUTION_MODE")
                    .env_remove("CARGO_TARGET_DIR")
                    .env_remove("RUST_TEST_THREADS");
            }),
        )?)
        .await
        .context("Failed to start rmcp client + ahma_mcp stdio")?;

    // Wait for roots/list handshake
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if roots_requested.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if !roots_requested.load(Ordering::SeqCst) {
        service.cancel().await.ok();
        anyhow::bail!("roots/list was never requested");
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test async execution (default mode)
    let params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "echo async_test",
                "working_directory": external_workspace_path.to_string_lossy()
                // Note: no execution_mode means AsyncResultPush (default)
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let req = ClientRequest::CallToolRequest(CallToolRequest::new(params));
    let response = service
        .send_request_with_option(
            req,
            rmcp::service::PeerRequestOptions {
                timeout: Some(Duration::from_secs(10)),
                meta: None,
            },
        )
        .await?
        .await_response()
        .await?;

    service.cancel().await.ok();

    match response {
        ServerResult::CallToolResult(result) => {
            if result.is_error == Some(true) {
                let error_text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");

                if error_text.contains("outside the sandbox")
                    || error_text.contains("filename")
                    || error_text.contains("No such file")
                {
                    anyhow::bail!(
                        "Async sandboxed_shell treated command as filename: {}",
                        error_text
                    );
                }

                anyhow::bail!("Async sandboxed_shell returned error: {}", error_text);
            }

            // For async mode, we should get an operation ID back
            let output_text = result
                .content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                .collect::<Vec<_>>()
                .join("\n");

            eprintln!("[Test] Async tool output: {}", output_text);

            // Should either contain operation ID or the actual output
            assert!(
                output_text.contains("op_")
                    || output_text.contains("async_test")
                    || output_text.contains("Asynchronous operation started"),
                "Async sandboxed_shell should return operation ID or output. Got: {}",
                output_text
            );

            Ok(())
        }
        other => {
            anyhow::bail!("Unexpected response type: {:?}", other);
        }
    }
}
