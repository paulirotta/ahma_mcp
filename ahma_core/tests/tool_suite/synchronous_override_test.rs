use ahma_core::test_utils as common;

use anyhow::Result;
use common::test_client;
use rmcp::model::CallToolRequestParams;
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

    // Baseline: without --sync flag, expect asynchronous (default is async)
    let baseline_client =
        test_client::new_client_in_dir(Some(&tools_dir_str), &[], temp_dir.path()).await?;
    let baseline_args = build_args("echo WITHOUT_OVERRIDE", &working_dir);
    let baseline_response = baseline_client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("test_sync"),
            arguments: Some(baseline_args),
            task: None,
            meta: None,
        })
        .await?;

    let baseline_text = baseline_response
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    // With async default and NO synchronous field in config, expect async output
    assert!(
        baseline_text.contains("Asynchronous operation started with ID"),
        "Tool without synchronous config should run async by default, got '{}'",
        baseline_text
    );

    baseline_client.cancel().await?;

    // With --sync flag, force sync mode for all tools
    let override_client =
        test_client::new_client_in_dir(Some(&tools_dir_str), &["--sync"], temp_dir.path()).await?;
    let override_args = build_args("echo WITH_SYNC_FLAG", &working_dir);
    let override_response = override_client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("test_sync"),
            arguments: Some(override_args),
            task: None,
            meta: None,
        })
        .await?;

    let override_text = override_response
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    // With --sync flag, should get synchronous output
    assert!(
        override_text.contains("WITH_SYNC_FLAG"),
        "Expected sync output with --sync flag, got '{}'",
        override_text
    );
    assert!(
        !override_text.contains("Asynchronous operation started with ID"),
        "Synchronous mode should not show async message, got '{}'",
        override_text
    );

    override_client.cancel().await?;
    Ok(())
}
