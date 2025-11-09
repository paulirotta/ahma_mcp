mod common;

use anyhow::Result;
use common::{templib, test_client};
use rmcp::model::CallToolRequestParam;
use serde_json::{json, Map};
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
    let temp_dir = templib::tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    fs::create_dir_all(&tools_dir).await?;

    let tool_config = r#"{
    "name": "test_async",
    "description": "Asynchronous test tool",
    "command": "bash -c",
    "enabled": true,
    "timeout_seconds": 60,
    "synchronous": false,
    "subcommand": [
        {
            "name": "default",
            "description": "Execute provided shell command asynchronously",
            "synchronous": false,
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

    fs::write(tools_dir.join("test_async.json"), tool_config).await?;

    let tools_dir_str = tools_dir.to_string_lossy().to_string();
    let working_dir = temp_dir.path().to_string_lossy().to_string();

    // Baseline: without --synchronous, expect async operation start message
    let baseline_client = test_client::new_client_with_args(Some(&tools_dir_str), &[]).await?;
    let baseline_args = build_args("echo WITHOUT_OVERRIDE", &working_dir);
    let baseline_response = baseline_client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("test_async"),
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
        baseline_text.contains("Asynchronous operation started with ID"),
        "Expected async start message without override, got '{}'",
        baseline_text
    );

    baseline_client.cancel().await?;

    // With --synchronous, expect direct command output instead of async message
    let override_client =
        test_client::new_client_with_args(Some(&tools_dir_str), &["--synchronous"]).await?;
    let override_args = build_args("echo WITH_OVERRIDE", &working_dir);
    let override_response = override_client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("test_async"),
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
        override_text.contains("WITH_OVERRIDE"),
        "Expected synchronous output to contain 'WITH_OVERRIDE', got '{}'",
        override_text
    );
    assert!(
        !override_text.contains("Asynchronous operation started with ID"),
        "Synchronous override should not report async start message, got '{}'",
        override_text
    );

    override_client.cancel().await?;
    Ok(())
}
