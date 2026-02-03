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

fn tools_dir_with_async_shell(temp: &TempDir) -> anyhow::Result<PathBuf> {
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
  "synchronous": false,
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

#[tokio::test]
async fn test_stdio_progress_notifications_respect_client_progress_token() -> anyhow::Result<()> {
    let temp = TempDir::new().context("Failed to create temp dir")?;
    let tools_dir = tools_dir_with_async_shell(&temp).context("Failed to create tools dir")?;

    let (tx, mut rx) = mpsc::channel::<ProgressNotificationParam>(128);
    let client_impl = RecordingClient {
        tx,
        roots: vec![temp.path().to_path_buf()],
    };

    let wd = workspace_dir();

    // Check for CARGO_TARGET_DIR
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| wd.join("target"));
    let binary_path = target_dir.join("debug/ahma_mcp");

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

    // NOTE: rmcp's send_request_with_option ALWAYS auto-assigns a progressToken via
    // ProgressTokenProvider, even when options.meta is None. There's no way to send
    // a request without a token through rmcp. This is by design - per MCP spec,
    // clients that want progress notifications should provide a token.
    //
    // Our test verifies that the server ECHOES the client's token correctly.

    // tools/call WITH explicit meta.progressToken
    let token_str = "tok_stdio_1";
    let token = ProgressToken(NumberOrString::String(Arc::from(token_str)));
    let mut meta = Meta::new();
    meta.set_progress_token(token.clone());

    let params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "sleep 0.2",
                "working_directory": &working_dir
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let req = ClientRequest::CallToolRequest(CallToolRequest::new(params));
    let handle = service
        .send_request_with_option(
            req,
            rmcp::service::PeerRequestOptions {
                timeout: Some(Duration::from_secs(5)),
                meta: Some(meta),
            },
        )
        .await?
        .await_response()
        .await?;

    // Ignore the call response; we only care about progress notifications.
    let _ = handle;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(p)) = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            assert_eq!(
                p.progress_token, token,
                "progressToken must match client token"
            );
            service.cancel().await.ok();
            return Ok(());
        }
    }

    service.cancel().await.ok();
    anyhow::bail!("did not observe notifications/progress with token {token_str}");
}
