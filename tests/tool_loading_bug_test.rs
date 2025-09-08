//! Tool Loading Compatibility Test
//!
//! PURPOSE: Validates JSON tool configuration loading after discovering critical bug
//! where adding extra JSON files (status.json, await.json) broke test expectations.
//!
//! LESSON LEARNED: Tool loading tests are brittle to configuration changes.
//! - Only external CLI tools should have JSON configurations
//! - Hardwired MCP tools (status, await) don't need JSON files  
//! - But users may add them anyway for documentation/IDE support
//!
//! MAINTENANCE: Update expected counts when adding new CLI tool integrations

use anyhow::Result;
use rmcp::model::CallToolRequestParam;

mod common;
use common::test_client::new_client;

/// TEST: Validates core CLI tool configurations are loaded
///
/// CRITICAL REQUIREMENT: Must load cargo.json, python3.json (minimum 2).
/// FLEXIBILITY: May also load status.json, await.json if user added them
///
/// LESSON LEARNED: Don't hard-code exact counts - use minimum expectations
/// This test previously failed when status.json/await.json were temporarily added
#[tokio::test]
async fn test_tools_are_loaded_after_json_migration() -> Result<()> {
    // Test that all JSON tool configurations are properly loaded
    let client = new_client(Some(".ahma/tools")).await?;

    let tools = client.list_tools(None).await?;

    // Should find tools from cargo.json and python3.json;
    assert!(
        tools.tools.len() >= 2,
        "Expected at least 2 tools loaded from JSON configs, but got {}. Tools found: {:?}",
        tools.tools.len(),
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Verify specific expected tools are present
    let tool_names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();

    // Should have cargo tool
    assert!(
        tool_names.iter().any(|name| name.starts_with("cargo")),
        "Expected cargo tool but found: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

/// TEST: Validates specific JSON tool actually functions
///
/// PURPOSE: Ensures loaded tools aren't just registered but actually callable.
/// Uses cargo version as test case since it's synchronous and always available.
///
/// MAINTENANCE: If cargo version is removed, update to another reliable synchronous tool
#[tokio::test]
async fn test_specific_json_tool_functionality() -> Result<()> {
    // Test that a specific tool from JSON config actually works
    let client = new_client(Some(".ahma/tools")).await?;

    let tools = client.list_tools(None).await?;

    // Find the 'cargo' tool to test
    let cargo_tool = tools.tools.iter().find(|t| t.name.as_ref() == "cargo");

    assert!(
        cargo_tool.is_some(),
        "cargo tool should be available from cargo.json config. Available tools: {:?}",
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Now, let's call 'cargo version'
    let params = serde_json::json!({
        "subcommand": "version"
    });

    let call_param = CallToolRequestParam {
        name: "cargo".into(),
        arguments: Some(params.as_object().unwrap().clone()),
    };

    let result = client.call_tool(call_param).await?;

    // Check if result contains expected content
    assert!(!result.content.is_empty(), "Result should have content");

    // Convert content to string for checking
    let content_str = if let Some(first_content) = result.content.first() {
        format!("{:?}", first_content)
    } else {
        String::new()
    };

    assert!(
        content_str.to_lowercase().contains("cargo"),
        "Expected cargo version output, got: {}",
        content_str
    );

    client.cancel().await?;
    Ok(())
}
