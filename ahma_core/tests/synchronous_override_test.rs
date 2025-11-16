mod common;

use anyhow::Result;
use common::test_client;
use rmcp::model::CallToolRequestParam;
use serde_json::{Map, json};
use std::borrow::Cow;
use tokio::fs;

fn build_args(command: &str, working_directory: &str) -> Map<String, serde_json::Value> {
    let mut args = Map::new();
    args.insert("command".to_string(), json!(command));
    args.insert(
        "working_directory".to_string(),
        json!(working_directory.to_string()),
    );
    args
}

#[tokio::test]
async fn test_synchronous_flag_overrides_async_tools() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    fs::create_dir_all(&tools_dir).await?;

    // Create a tool that defaults to synchronous (no asynchronous field)
    let tool_config = r#"{
    "name": "test_sync",
    "description": "Synchronous test tool",
    "command": "bash -c",
    "enabled": true,
    "timeout_seconds": 60,
    "subcommand": [
        {
            "name": "default",
            "description": "Execute provided shell command synchronously by default",
            "positional_args": [
                {
                    "name": "command",
                    "type": "string",
                    "description": "Shell command to execute",
                    "required": true
                }
            ]
        }
    ]
}"#;

    fs::write(tools_dir.join("test_sync.json"), tool_config).await?;

    let tools_dir_str = tools_dir.to_string_lossy().to_string();
    let working_dir = temp_dir.path().to_string_lossy().to_string();

    // Baseline: without --asynchronous flag, expect synchronous (direct output)
    let baseline_client = test_client::new_client_with_args(Some(&tools_dir_str), &[]).await?;
    let baseline_args = build_args("echo WITHOUT_OVERRIDE", &working_dir);
    let baseline_response = baseline_client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("test_sync"),
            arguments: Some(baseline_args),
        })
        .await?;

    let baseline_text = baseline_response
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    assert!(
        baseline_text.contains("WITHOUT_OVERRIDE"),
        "Expected synchronous output without override, got '{}'",
        baseline_text
    );
    assert!(
        !baseline_text.contains("Asynchronous operation started with ID"),
        "Should not get async message by default, got '{}'",
        baseline_text
    );

    baseline_client.cancel().await?;

    // With --async flag, force async mode for all tools
    let override_client =
        test_client::new_client_with_args(Some(&tools_dir_str), &["--async"]).await?;
    let override_args = build_args("echo WITH_OVERRIDE", &working_dir);
    let override_response = override_client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("test_sync"),
            arguments: Some(override_args),
        })
        .await?;

    let override_text = override_response
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    assert!(
        override_text.contains("Asynchronous operation started with ID"),
        "Expected async start message with --async flag, got '{}'",
        override_text
    );
    assert!(
        !override_text.contains("WITH_OVERRIDE"),
        "Asynchronous mode should not show direct output, got '{}'",
        override_text
    );

    override_client.cancel().await?;
    Ok(())
}
