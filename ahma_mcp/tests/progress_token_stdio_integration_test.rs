use anyhow::Context;
use rmcp::{
    ServiceExt,
    model::{
        CallToolRequest, CallToolRequestParams, ClientCapabilities, ClientRequest, ClientResult,
        Implementation, InitializeRequestParams, ListRootsResult, Meta, NumberOrString,
        ProgressNotificationParam, ProgressToken, ProtocolVersion, Root, ServerNotification,
        ServerRequest,
    },
    service::{NotificationContext, RequestContext, RoleClient},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use std::borrow::Cow;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
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

/// Create a SYNCHRONOUS shell tool configuration.
///
/// CRITICAL: For testing progress notifications in stdio transport, we MUST use
/// synchronous execution. Here's why:
///
/// With async execution:
/// 1. Server sends "Started" progress notification
/// 2. Server immediately returns "Async operation started" response
/// 3. Client receives response and may begin transport teardown
/// 4. "Started" notification may never be delivered because transport is closing
///
/// With sync execution:
/// 1. Server starts processing and sends "Started" notification
/// 2. Server waits for command to complete (echo is instant)
/// 3. Server sends response only after command completes
/// 4. All notifications are delivered BEFORE the response
///
/// This ordering guarantees the client receives notifications before the response
/// that would trigger test completion.
fn tools_dir_with_sync_shell(temp: &TempDir) -> anyhow::Result<PathBuf> {
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

#[derive(Clone)]
struct RecordingClient {
    tx: mpsc::Sender<ProgressNotificationParam>,
    roots: Vec<PathBuf>,
}

#[allow(clippy::manual_async_fn)]
impl rmcp::service::Service<RoleClient> for RecordingClient {
    fn get_info(&self) -> InitializeRequestParams {
        InitializeRequestParams {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "progress-token-test-client".into(),
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
        async move {
            if let ServerNotification::ProgressNotification(n) = notification {
                // Log receipt for debugging test flakes: include token and timestamp.
                use std::time::SystemTime;
                eprintln!(
                    "[TEST_CLIENT] RECV_PROGRESS: {:?} | ts: {:?}",
                    n.params.progress_token,
                    SystemTime::now()
                );
                let _ = self.tx.send(n.params).await;
            }
            Ok(())
        }
    }
}

/// Test that progress notifications contain the client-provided progressToken.
///
/// This test verifies R2.2 from REQUIREMENTS.md: "On completion, the system must
/// push results via MCP progress notifications."
///
/// ## CI-Resilience Design
///
/// This test uses synchronous tool execution to avoid race conditions between
/// notification delivery and transport teardown. See `tools_dir_with_sync_shell()`
/// for detailed rationale.
///
/// The test:
/// 1. Creates a recording client that captures progress notifications
/// 2. Sends a synchronous tool call with an explicit progressToken
/// 3. Spawns the response awaiting in a background task
/// 4. Waits for a notification with matching token (with generous timeout)
/// 5. Verifies the token matches what the client sent
#[tokio::test]
async fn test_stdio_progress_notifications_respect_client_progress_token() -> anyhow::Result<()> {
    let temp = TempDir::new().context("Failed to create temp dir")?;
    // Use SYNCHRONOUS shell to ensure notifications are sent before response
    let tools_dir = tools_dir_with_sync_shell(&temp).context("Failed to create tools dir")?;

    let (tx, mut rx) = mpsc::channel::<ProgressNotificationParam>(128);
    let client_impl = RecordingClient {
        tx,
        roots: vec![temp.path().to_path_buf(), workspace_dir()],
    };

    let wd = workspace_dir();

    let binary_path = ahma_mcp::test_utils::cli::get_binary_path("ahma_mcp", "ahma_mcp");

    // Create a small wrapper script that tees the child's stdout to stderr so
    // the test process can observe raw JSON-RPC lines even if the client handler
    // misses them. This avoids changing rmcp internals.
    let wrapper_path = temp.path().join("child_wrapper.sh");
    std::fs::write(
        &wrapper_path,
        r#"#!/bin/sh
# Run the target binary (first arg) with remaining args, teeing stdout to stderr
"#,
    )?;
    // Append the execution lines in append mode so we can include the exec logic
    use std::fs::OpenOptions;
    {
        use std::io::Write;
        let mut f = OpenOptions::new().append(true).open(&wrapper_path)?;
        writeln!(f, r#"exec "$@" | tee /dev/stderr"#)?;
    } // `f` dropped here to ensure the writer FD is closed before we spawn the wrapper
    // Make the wrapper executable
    let mut perms = std::fs::metadata(&wrapper_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper_path, perms)?;

    let service = client_impl
        .clone()
        .serve(TokioChildProcess::new(
            Command::new(wrapper_path).configure(|cmd| {
                cmd.arg(&binary_path)
                    .arg("--tools-dir")
                    .arg(&tools_dir)
                    .arg("--log-to-stderr")
                    .current_dir(&wd)
                    // AHMA_TEST_MODE=1 disables path validation (allows temp dirs)
                    // This is appropriate since we're testing progress token behavior, not sandbox
                    .env("AHMA_TEST_MODE", "1")
                    .env_remove("NEXTEST")
                    .env_remove("NEXTEST_EXECUTION_MODE")
                    .env_remove("CARGO_TARGET_DIR")
                    .env_remove("RUST_TEST_THREADS");
            }),
        )?)
        .await
        .context("Failed to start rmcp client + ahma_mcp stdio")?;

    // Yield to allow initialization to complete without timing sleeps
    tokio::task::yield_now().await;

    // Use workspace directory for working_directory (inside sandbox scope)
    let working_dir = wd.to_string_lossy().to_string();

    // tools/call WITH explicit meta.progressToken
    let token_str = "tok_stdio_1";
    let token = ProgressToken(NumberOrString::String(Arc::from(token_str)));
    let mut meta = Meta::new();
    meta.set_progress_token(token.clone());

    // Use a fast synchronous command (echo is instant)
    let params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "sleep 1; echo 'progress test'",
                "working_directory": &working_dir,
                "execution_mode": "Synchronous"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let req = ClientRequest::CallToolRequest(CallToolRequest::new(params));

    // Send the request and spawn response handling in background.
    // For synchronous execution, notifications are sent BEFORE the response,
    // so we should receive the notification before the response task completes.
    let pending_response = service
        .send_request_with_option(
            req,
            rmcp::service::PeerRequestOptions {
                timeout: Some(Duration::from_secs(30)),
                meta: Some(meta),
            },
        )
        .await?;

    // Spawn response handling so it doesn't block notification reception
    let response_handle = tokio::spawn(async move { pending_response.await_response().await });

    // Wait for notification with a generous timeout for CI environments.
    // The CI_NOTIFICATION_TIMEOUT (30s) is appropriate for heavily-loaded CI servers
    // and coverage builds (llvm-cov adds significant overhead).
    let notification_timeout = Duration::from_secs(30);

    // CRITICAL: We must wait for both the notification AND the response to complete
    // before cancelling the service. If we cancel while the tool is in flight,
    // the adapter shuts down and will never send the progress notification.
    //
    // Use tokio::select! to race between notification and timeout, while also
    // ensuring the response task continues running in the background.
    let notification_result = tokio::time::timeout(notification_timeout, async {
        // For synchronous tool execution, the notification is sent before the response,
        // so we should receive it first.
        match rx.recv().await {
            Some(p) => {
                eprintln!(
                    "[TEST] Received notification with token: {:?}",
                    p.progress_token
                );
                Some(p)
            }
            None => {
                // Channel closed - sender dropped
                eprintln!("[TEST] Notification channel closed unexpectedly");
                None
            }
        }
    })
    .await;

    // Wait for response task to complete BEFORE cancelling the service.
    // This ensures the tool has fully completed execution and had a chance
    // to send all notifications before we tear down the transport.
    let _ = response_handle.await;

    // Now cancel the service (cleanup)
    service.cancel().await.ok();

    match notification_result {
        Ok(Some(p)) => {
            assert_eq!(
                p.progress_token, token,
                "progressToken must match client token. Expected {:?}, got {:?}",
                token, p.progress_token
            );
            Ok(())
        }
        Ok(None) => {
            anyhow::bail!(
                "Notification channel closed without receiving any notifications. \
                 This indicates the client's handle_notification was never called. \
                 Check that the server is configured for synchronous execution."
            )
        }
        Err(_timeout) => {
            anyhow::bail!(
                "Timeout ({:?}) waiting for notifications/progress with token '{}'. \
                 This may indicate a race condition in the test or server not sending notifications. \
                 Check server logs (--log-to-stderr) for 'notifications/progress' sends.",
                notification_timeout,
                token_str
            )
        }
    }
}
