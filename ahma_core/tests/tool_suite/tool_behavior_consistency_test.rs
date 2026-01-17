//! TDD tests for tool configuration and execution issues
use ahma_core::{
    skip_if_disabled_async_result, test_utils as common, utils::logging::init_test_logging,
};
use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

#[tokio::test]
async fn test_synchronous_cargo_check_returns_actual_results() -> Result<()> {
    init_test_logging();
    skip_if_disabled_async_result!("cargo");
    // This test identifies the issue where cargo check should return actual results

    let client = new_client(Some(".ahma")).await?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("cargo"),
        arguments: Some(serde_json::from_value(json!({ "subcommand": "check" })).unwrap()),
        task: None,
    };

    let result = client.call_tool(call_param).await?;

    // Cargo check should return actual results
    assert!(
        !result.content.is_empty(),
        "Cargo check should return non-empty results"
    );

    // Check that we get actual cargo output
    let content_text = result
        .content
        .iter()
        .find_map(|c| c.as_text())
        .expect("Should have text content");

    // Should contain actual cargo check output
    assert!(
        content_text.text.contains("Finished")
            || content_text.text.contains("Checking")
            || content_text.text.contains("error")
            || content_text.text.contains("warning"),
        "Cargo check should return compilation results, got: {}",
        content_text.text
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_ls_tool_command_structure() -> Result<()> {
    init_test_logging();
    // This test identifies the issue where ls_ls tool fails with command structure
    // The tool appears to be running "ls ls" instead of just "ls"

    let client = new_client(Some(".ahma")).await?;

    // Check if ls tool is available (optional since ls.json was removed)
    let tools = client.list_tools(None).await?;
    let has_ls_tool = tools.tools.iter().any(|t| t.name.as_ref() == "ls_default");

    if !has_ls_tool {
        println!("Skipping test: ls tool not available (ls.json removed)");
        return Ok(());
    }

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("ls_default"),
        arguments: None, // ls without arguments should list current directory
        task: None,
    };

    let result = client.call_tool(call_param).await?;

    // The issue: ls command should work, not fail with "No such file or directory"
    assert!(
        !result.content.is_empty(),
        "ls_default should return results"
    );

    let content_text = result
        .content
        .iter()
        .find_map(|c| c.as_text())
        .expect("Should have text content");

    // Should not contain command structure errors
    assert!(
        !content_text.text.contains("No such file or directory")
            && !content_text.text.contains("unrecognized option"),
        "ls command should not fail with command structure errors, got: {}",
        content_text.text
    );

    // Should contain actual directory listing or meaningful ls output
    assert!(
        content_text.text.contains("Cargo.toml")
            || content_text.text.contains("src")
            || content_text.text.len() > 5, // Some directory output
        "ls should return meaningful directory listing, got: {}",
        content_text.text
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_tool_descriptions_match_actual_behavior() -> Result<()> {
    init_test_logging();
    skip_if_disabled_async_result!("cargo");
    // This test identifies inconsistencies between tool descriptions and actual behavior
    // Specifically checking if synchronous tools are properly described

    let client = new_client(Some(".ahma")).await?;
    let tools_result = client.list_tools(None).await?;

    // Find cargo tool and verify its description
    let cargo_tool = tools_result
        .tools
        .iter()
        .find(|t| t.name.as_ref() == "cargo")
        .expect("cargo tool should exist");

    assert!(
        cargo_tool.description.is_some(),
        "Cargo tool should have a description"
    );
    assert!(
        cargo_tool
            .description
            .as_ref()
            .unwrap()
            .contains("Rust's build tool"),
        "Cargo tool description is incorrect"
    );

    client.cancel().await?;
    Ok(())
}

/// Test that sandboxed_shell (always available) returns actual results.
/// This ensures core tool execution functionality is always tested.
#[tokio::test]
async fn test_sandboxed_shell_returns_actual_results() -> Result<()> {
    init_test_logging();
    // sandboxed_shell is always available - no skip needed

    let client = new_client(Some(".ahma")).await?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "echo 'test output from sandboxed_shell'",
                "execution_mode": "Synchronous"
            }))
            .unwrap(),
        ),
        task: None,
    };

    let result = client.call_tool(call_param).await?;

    // Should return actual results
    assert!(
        !result.content.is_empty(),
        "sandboxed_shell should return non-empty results"
    );

    // Check that we get actual shell output
    let content_text = result
        .content
        .iter()
        .find_map(|c| c.as_text())
        .expect("Should have text content");

    assert!(
        content_text
            .text
            .contains("test output from sandboxed_shell"),
        "sandboxed_shell should return expected output, got: {}",
        content_text.text
    );

    client.cancel().await?;
    Ok(())
}
