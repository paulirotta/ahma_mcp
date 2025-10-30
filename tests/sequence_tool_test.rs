//! Integration tests for sequence tools - composite tools that execute multiple steps
mod common;

use ahma_mcp::utils::logging::init_test_logging;
use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

#[tokio::test]
async fn test_sequence_tool_loads() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let tools = client.list_tools(None).await?;
    let tool_names: Vec<_> = tools.tools.iter().map(|t| t.name.as_ref()).collect();

    // Verify rust_quality_check tool is loaded
    assert!(
        tool_names.contains(&"rust_quality_check"),
        "rust_quality_check tool should be loaded. Available tools: {:?}",
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
        "synchronous": true,
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
        ".ahma/tools/shell_async.json",
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

    // Verify the result contains information about both steps
    let result_text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result_text.contains("Step 1") || result_text.contains("First step"),
        "Result should contain first step: {}",
        result_text
    );
    assert!(
        result_text.contains("Step 2") || result_text.contains("Second step"),
        "Result should contain second step: {}",
        result_text
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_rust_quality_check_structure() -> Result<()> {
    init_test_logging();

    // Test that the rust_quality_check.json file is valid
    let config_path = std::path::Path::new(".ahma/tools/rust_quality_check.json");
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
        "Command should be 'sequence'"
    );
    assert!(config["sequence"].is_array(), "Should have sequence array");

    let sequence = config["sequence"].as_array().unwrap();
    assert_eq!(
        sequence.len(),
        4,
        "Should have 4 steps: fmt, clippy, nextest, build"
    );

    // Verify each step
    assert_eq!(sequence[0]["tool"].as_str(), Some("cargo_fmt"));
    assert_eq!(sequence[0]["subcommand"].as_str(), Some("default"));

    assert_eq!(sequence[1]["tool"].as_str(), Some("cargo_clippy"));
    assert_eq!(sequence[1]["subcommand"].as_str(), Some("clippy"));

    assert_eq!(sequence[2]["tool"].as_str(), Some("cargo_nextest"));
    assert_eq!(sequence[2]["subcommand"].as_str(), Some("nextest_run"));

    assert_eq!(sequence[3]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[3]["subcommand"].as_str(), Some("build"));

    // Verify delay
    assert_eq!(
        config["step_delay_ms"].as_u64(),
        Some(100),
        "Step delay should be 100ms"
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
        "synchronous": true,
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

    let result = client.call_tool(call_param).await?;

    // The sequence should handle the error gracefully
    let result_text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        result_text.contains("not found") || result_text.contains("failed"),
        "Result should indicate the tool was not found: {}",
        result_text
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
        "synchronous": true,
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
        ".ahma/tools/shell_async.json",
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

    // Verify all steps executed
    let result_text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(result_text.contains("Step A"));
    assert!(result_text.contains("Step B"));
    assert!(result_text.contains("Step C"));

    client.cancel().await?;
    Ok(())
}
