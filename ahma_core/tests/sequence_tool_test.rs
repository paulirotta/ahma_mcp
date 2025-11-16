//! Integration tests for sequence tools - composite tools that execute multiple steps
mod common;

use crate::common::{get_workspace_path, test_client::new_client};
use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;
use std::collections::HashSet;

#[tokio::test]
async fn test_sequence_tool_loads() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let tools = client.list_tools(None).await?;
    let tool_names: Vec<_> = tools.tools.iter().map(|t| t.name.as_ref()).collect();

    // Verify cargo tool is loaded (it now contains the quality-check subcommand sequence)
    assert!(
        tool_names.contains(&"cargo"),
        "cargo tool should be loaded. Available tools: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_simple_sequence_execution() -> Result<()> {
    init_test_logging();

    // Create a temporary directory with a simple test tool configuration
    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    // Create a simple sequence tool that runs echo commands
    let simple_sequence = json!({
        "name": "test_sequence",
        "description": "Test sequence with echo commands",
        "command": "sequence",
        "enabled": true,
        "asynchronous": false,
        "timeout_seconds": 30,
        "sequence": [
            {
                "tool": "shell_async",
                "subcommand": "default",
                "args": {
                    "command": "echo 'Step 1'"
                },
                "description": "First step"
            },
            {
                "tool": "shell_async",
                "subcommand": "default",
                "args": {
                    "command": "echo 'Step 2'"
                },
                "description": "Second step"
            }
        ],
        "step_delay_ms": 100
    });

    // Copy shell_async.json to the test tools directory
    std::fs::copy(
        get_workspace_path(".ahma/tools/shell_async.json"),
        tools_dir.join("shell_async.json"),
    )?;

    std::fs::write(
        tools_dir.join("test_sequence.json"),
        serde_json::to_string_pretty(&simple_sequence)?,
    )?;

    let client = new_client(Some(tools_dir.to_str().unwrap())).await?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("test_sequence"),
        arguments: Some(
            json!({
                "working_directory": temp_dir.path().to_str().unwrap()
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await?;

    // Verify the result contains information about both steps including their descriptions and operation IDs
    let messages: Vec<String> = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();

    assert_eq!(
        messages.len(),
        2,
        "Sequence should emit exactly two step notifications: {:?}",
        messages
    );

    let expected_descriptions = ["First step", "Second step"];
    for (message, expected_desc) in messages.iter().zip(expected_descriptions.iter()) {
        assert!(
            message.contains("Sequence step 'shell_async'"),
            "Message should reference shell_async sequence step: {}",
            message
        );
        assert!(
            message.contains(expected_desc),
            "Message should include step description '{}': {}",
            expected_desc,
            message
        );
        assert!(
            message.contains("operation ID:"),
            "Message should include operation ID: {}",
            message
        );
    }

    let operation_ids: HashSet<String> = messages
        .iter()
        .filter_map(|line| line.split("operation ID:").nth(1))
        .map(|id| id.trim().to_string())
        .collect();
    assert_eq!(
        operation_ids.len(),
        messages.len(),
        "Each sequence step should have a unique operation ID: {:?}",
        operation_ids
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_rust_quality_check_structure() -> Result<()> {
    init_test_logging();

    // Test that the rust_quality_check.json file contains the dedicated sequence
    let config_path = get_workspace_path(".ahma/tools/rust_quality_check.json");
    assert!(config_path.exists(), "rust_quality_check.json should exist");

    let config_content = std::fs::read_to_string(config_path)?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    // Verify structure
    assert_eq!(
        config["name"].as_str(),
        Some("rust_quality_check"),
        "Tool name should be rust_quality_check"
    );
    assert_eq!(
        config["command"].as_str(),
        Some("sequence"),
        "rust_quality_check should be a sequence tool"
    );

    // Sequence tools have top-level sequence, not subcommand-level
    assert!(
        config["sequence"].is_array(),
        "Should have top-level sequence array for cross-tool orchestration"
    );

    let sequence = config["sequence"].as_array().unwrap();
    assert_eq!(
        sequence.len(),
        7,
        "Should have 7 steps: schema generation, validation, fmt, clippy, clippy tests, nextest, build"
    );

    // Verify each step
    assert_eq!(sequence[0]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[0]["subcommand"].as_str(), Some("run"));
    assert_eq!(
        sequence[0]["args"]["bin"].as_str(),
        Some("generate_tool_schema"),
        "First step should regenerate the schema"
    );
    assert_eq!(sequence[1]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[1]["subcommand"].as_str(), Some("run"));
    assert_eq!(
        sequence[1]["args"]["bin"].as_str(),
        Some("ahma_validate"),
        "Second step should run ahma_validate"
    );

    assert_eq!(sequence[2]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[2]["subcommand"].as_str(), Some("fmt"));
    assert_eq!(sequence[2]["args"]["all"].as_bool(), Some(true));

    assert_eq!(sequence[3]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[3]["subcommand"].as_str(), Some("clippy"));
    assert_eq!(sequence[3]["args"]["fix"].as_bool(), Some(true));
    assert_eq!(sequence[3]["args"]["allow-dirty"].as_bool(), Some(true));
    assert_eq!(sequence[3]["args"]["workspace"].as_bool(), Some(true));

    assert_eq!(sequence[4]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[4]["subcommand"].as_str(), Some("clippy"));
    assert_eq!(sequence[4]["args"]["fix"].as_bool(), Some(true));
    assert_eq!(sequence[4]["args"]["tests"].as_bool(), Some(true));
    assert_eq!(sequence[4]["args"]["allow-dirty"].as_bool(), Some(true));
    assert_eq!(sequence[4]["args"]["workspace"].as_bool(), Some(true));

    assert_eq!(sequence[5]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[5]["subcommand"].as_str(), Some("nextest_run"));
    assert_eq!(sequence[5]["args"]["workspace"].as_bool(), Some(true));

    assert_eq!(sequence[6]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[6]["subcommand"].as_str(), Some("build"));
    assert_eq!(sequence[6]["args"]["workspace"].as_bool(), Some(true));

    // Verify delay
    assert_eq!(
        config["step_delay_ms"].as_u64(),
        Some(500),
        "Step delay should be 500ms"
    );

    Ok(())
}

#[tokio::test]
async fn test_sequence_with_invalid_tool() -> Result<()> {
    init_test_logging();

    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    // Create a sequence tool that references a non-existent tool
    let invalid_sequence = json!({
        "name": "invalid_sequence",
        "description": "Sequence with invalid tool",
        "command": "sequence",
        "enabled": true,
        "asynchronous": false,
        "timeout_seconds": 30,
        "sequence": [
            {
                "tool": "nonexistent_tool",
                "subcommand": "test",
                "args": {},
                "description": "This should fail"
            }
        ],
        "step_delay_ms": 100
    });

    std::fs::write(
        tools_dir.join("invalid_sequence.json"),
        serde_json::to_string_pretty(&invalid_sequence)?,
    )?;

    let client = new_client(Some(tools_dir.to_str().unwrap())).await?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("invalid_sequence"),
        arguments: Some(
            json!({
                "working_directory": temp_dir.path().to_str().unwrap()
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let error = client
        .call_tool(call_param)
        .await
        .expect_err("Sequence should fail because the tool is missing");

    let error_text = format!("{error:?}");
    assert!(
        error_text.contains("nonexistent_tool"),
        "Error should reference the missing tool: {}",
        error_text
    );
    assert!(
        error_text.contains("not configured"),
        "Error should indicate the tool is not configured: {}",
        error_text
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_sequence_delay_is_applied() -> Result<()> {
    init_test_logging();

    use std::time::Instant;

    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    // Create a sequence with multiple quick steps
    let timed_sequence = json!({
        "name": "timed_sequence",
        "description": "Sequence to test timing",
        "command": "sequence",
        "enabled": true,
        "asynchronous": false,
        "timeout_seconds": 30,
        "sequence": [
            {
                "tool": "shell_async",
                "subcommand": "default",
                "args": {
                    "command": "echo 'A'"
                },
                "description": "Step A"
            },
            {
                "tool": "shell_async",
                "subcommand": "default",
                "args": {
                    "command": "echo 'B'"
                },
                "description": "Step B"
            },
            {
                "tool": "shell_async",
                "subcommand": "default",
                "args": {
                    "command": "echo 'C'"
                },
                "description": "Step C"
            }
        ],
        "step_delay_ms": 100
    });

    std::fs::copy(
        get_workspace_path(".ahma/tools/shell_async.json"),
        tools_dir.join("shell_async.json"),
    )?;

    std::fs::write(
        tools_dir.join("timed_sequence.json"),
        serde_json::to_string_pretty(&timed_sequence)?,
    )?;

    let client = new_client(Some(tools_dir.to_str().unwrap())).await?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("timed_sequence"),
        arguments: Some(
            json!({
                "working_directory": temp_dir.path().to_str().unwrap()
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let start = Instant::now();
    let result = client.call_tool(call_param).await?;
    let duration = start.elapsed();

    // With 3 steps and 100ms delay between them, we should have at least 200ms total
    // (2 delays: between step 1-2 and step 2-3)
    assert!(
        duration.as_millis() >= 200,
        "Execution should take at least 200ms (got {}ms)",
        duration.as_millis()
    );

    // Verify all steps emitted notifications with their descriptions and IDs
    let result_text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");

    for expected_desc in ["Step A", "Step B", "Step C"] {
        assert!(
            result_text.contains(expected_desc),
            "Result should include step description '{}': {}",
            expected_desc,
            result_text
        );
    }
    assert!(
        result_text.contains("Sequence step 'shell_async'"),
        "Result should mention shell_async sequence steps: {}",
        result_text
    );
    assert!(
        result_text.contains("operation ID:"),
        "Result should include operation IDs: {}",
        result_text
    );

    client.cancel().await?;
    Ok(())
}
