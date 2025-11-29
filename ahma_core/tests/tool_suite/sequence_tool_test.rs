//! Integration tests for sequence tools - composite tools that execute multiple steps

use ahma_core::test_utils::get_workspace_path;
use ahma_core::test_utils::test_client::{new_client, new_client_in_dir};
use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

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
        "force_synchronous": true,
        "timeout_seconds": 30,
        "sequence": [
            {
                "tool": "sandboxed_shell",
                "subcommand": "default",
                "args": {
                    "command": "echo 'Step 1'"
                },
                "description": "First step"
            },
            {
                "tool": "sandboxed_shell",
                "subcommand": "default",
                "args": {
                    "command": "echo 'Step 2'"
                },
                "description": "Second step"
            }
        ],
        "step_delay_ms": 100
    });

    // Copy sandboxed_shell.json to the test tools directory
    std::fs::copy(
        get_workspace_path(".ahma/tools/sandboxed_shell.json"),
        tools_dir.join("sandboxed_shell.json"),
    )?;

    std::fs::write(
        tools_dir.join("test_sequence.json"),
        serde_json::to_string_pretty(&simple_sequence)?,
    )?;

    let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path()).await?;

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

    // With synchronous sequence execution (synchronous: true),
    // all results come in a single combined message showing all steps
    let messages: Vec<String> = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();

    assert_eq!(
        messages.len(),
        1,
        "Synchronous sequence should emit single combined result: {:?}",
        messages
    );

    let combined_message = &messages[0];

    // Verify the combined message contains information about all steps
    assert!(
        combined_message.contains("All 2 sequence steps completed successfully"),
        "Message should indicate all steps completed: {}",
        combined_message
    );

    assert!(
        combined_message.contains("Step 1") && combined_message.contains("Step 2"),
        "Message should include step output: {}",
        combined_message
    );

    assert!(
        combined_message.contains("sandboxed_shell"),
        "Message should reference sandboxed_shell tool: {}",
        combined_message
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_cargo_qualitycheck_structure() -> Result<()> {
    init_test_logging();

    // Test that the cargo.json file contains the qualitycheck subcommand sequence
    let config_path = get_workspace_path(".ahma/tools/cargo.json");
    assert!(config_path.exists(), "cargo.json should exist");

    let config_content = std::fs::read_to_string(config_path)?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    // Verify top-level structure
    assert_eq!(
        config["name"].as_str(),
        Some("cargo"),
        "Tool name should be cargo"
    );
    assert_eq!(
        config["command"].as_str(),
        Some("cargo"),
        "cargo should use cargo command"
    );

    // Find the qualitycheck subcommand
    let subcommands = config["subcommand"]
        .as_array()
        .expect("cargo should have subcommands array");

    let qualitycheck = subcommands
        .iter()
        .find(|s| s["name"].as_str() == Some("qualitycheck"))
        .expect("cargo should have qualitycheck subcommand");

    // Subcommand sequences are at the subcommand level, not top-level
    assert!(
        qualitycheck["sequence"].is_array(),
        "Should have subcommand-level sequence array within qualitycheck"
    );

    let sequence = qualitycheck["sequence"].as_array().unwrap();
    assert_eq!(
        sequence.len(),
        5,
        "Generic qualitycheck should have 5 steps: fmt, clippy, clippy tests, nextest, build (no schema generation or validation)"
    );

    // Verify each step
    assert_eq!(sequence[0]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[0]["subcommand"].as_str(), Some("fmt"));
    assert_eq!(sequence[0]["args"]["all"].as_bool(), Some(true));

    assert_eq!(sequence[1]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[1]["subcommand"].as_str(), Some("clippy"));
    assert_eq!(sequence[1]["args"]["fix"].as_bool(), Some(true));
    assert_eq!(sequence[1]["args"]["allow-dirty"].as_bool(), Some(true));
    assert_eq!(sequence[1]["args"]["workspace"].as_bool(), Some(true));

    assert_eq!(sequence[2]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[2]["subcommand"].as_str(), Some("clippy"));
    assert_eq!(sequence[2]["args"]["fix"].as_bool(), Some(true));
    assert_eq!(sequence[2]["args"]["tests"].as_bool(), Some(true));
    assert_eq!(sequence[2]["args"]["allow-dirty"].as_bool(), Some(true));
    assert_eq!(sequence[2]["args"]["workspace"].as_bool(), Some(true));

    assert_eq!(sequence[3]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[3]["subcommand"].as_str(), Some("nextest_run"));
    assert_eq!(sequence[3]["args"]["workspace"].as_bool(), Some(true));

    assert_eq!(sequence[4]["tool"].as_str(), Some("cargo"));
    assert_eq!(sequence[4]["subcommand"].as_str(), Some("build"));
    assert_eq!(sequence[4]["args"]["workspace"].as_bool(), Some(true));

    // Verify delay
    assert_eq!(
        qualitycheck["step_delay_ms"].as_u64(),
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
        "force_synchronous": true,
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
        .expect_err("Sequence should fail because the referenced tool is missing");

    // The error can be either "Tool not found" (if the sequence tool itself isn't loaded)
    // or "not configured" (if the sequence loads but references a missing tool)
    let error_text = format!("{error:?}");
    let has_tool_reference =
        error_text.contains("nonexistent_tool") || error_text.contains("invalid_sequence");
    let has_error_indication =
        error_text.contains("not configured") || error_text.contains("not found");

    assert!(
        has_tool_reference && has_error_indication,
        "Error should reference the missing tool and indicate it's not available: {}",
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
        "force_synchronous": true,
        "timeout_seconds": 30,
        "sequence": [
            {
                "tool": "sandboxed_shell",
                "subcommand": "default",
                "args": {
                    "command": "echo 'A'"
                },
                "description": "Step A"
            },
            {
                "tool": "sandboxed_shell",
                "subcommand": "default",
                "args": {
                    "command": "echo 'B'"
                },
                "description": "Step B"
            },
            {
                "tool": "sandboxed_shell",
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
        get_workspace_path(".ahma/tools/sandboxed_shell.json"),
        tools_dir.join("sandboxed_shell.json"),
    )?;

    std::fs::write(
        tools_dir.join("timed_sequence.json"),
        serde_json::to_string_pretty(&timed_sequence)?,
    )?;

    let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path()).await?;

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

    // Verify the synchronous sequence completed with all step outputs
    let result_text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");

    // Synchronous sequences return all results in one combined message
    assert!(
        result_text.contains("All 3 sequence steps completed successfully"),
        "Result should indicate all steps completed: {}",
        result_text
    );

    // Verify all step outputs are present (the echo output "A", "B", "C")
    for expected_output in ["A", "B", "C"] {
        assert!(
            result_text.contains(expected_output),
            "Result should include step output '{}': {}",
            expected_output,
            result_text
        );
    }

    assert!(
        result_text.contains("sandboxed_shell"),
        "Result should mention sandboxed_shell tool: {}",
        result_text
    );

    client.cancel().await?;
    Ok(())
}
