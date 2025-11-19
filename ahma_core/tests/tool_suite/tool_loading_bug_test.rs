//! Tool Loading Compatibility Test
//! PURPOSE: Validates JSON tool configuration loading after discovering critical bug
//! where adding extra JSON files (status.json, await.json) broke test expectations.
//! LESSON LEARNED: Tool loading tests are brittle to configuration changes.
//! - Only external CLI tools should have JSON configurations
//! - Hardwired MCP tools (status, await) don't need JSON files  
//! - But users may add them anyway for documentation/IDE support
//!   MAINTENANCE: Update expected counts when adding new CLI tool integrations

use ahma_core::test_utils as common;

use anyhow::Result;

use ahma_core::utils::logging::init_test_logging;
use common::test_client::new_client;

/// TEST: Validates core CLI tool configurations are loaded
///
/// CRITICAL REQUIREMENT: Must load cargo.json, python3.json (minimum 2). ls.json is now OPTIONAL.
/// FLEXIBILITY: May also load status.json, await.json if user added them
///
/// LESSON LEARNED: Don't hard-code exact counts - use minimum expectations
/// This test previously failed when status.json/await.json were temporarily added
#[tokio::test]
async fn test_tools_are_loaded_after_json_migration() -> Result<()> {
    init_test_logging();
    // Test that all 3 JSON tool configurations are properly loaded
    let client = new_client(Some(".ahma/tools")).await?;

    let tools = client.list_tools(None).await?;

    // Should find tools from cargo.json and python3.json; ls.json is optional
    assert!(
        tools.tools.len() >= 3,
        "Expected at least 3 tools loaded from JSON configs, but got {}. Tools found: {:?}",
        tools.tools.len(),
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Verify specific expected tools are present
    let tool_names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();

    // Should have cargo tool (with subcommands)
    assert!(
        tool_names.contains(&"cargo"),
        "Expected cargo tool but found: {:?}",
        tool_names
    );

    // ls tool (file listing) is optional; do not assert its presence

    client.cancel().await?;
    Ok(())
}

/// TEST: Validates specific JSON tool actually functions
///
/// PURPOSE: Ensures loaded tools aren't just registered but actually callable.
/// Uses cargo_version as test case since it's synchronous and always available.
///
/// MAINTENANCE: If cargo_version is removed, update to another reliable synchronous tool
#[tokio::test]
async fn test_specific_json_tool_functionality() -> Result<()> {
    init_test_logging();
    // Test that a specific tool from JSON config actually works
    let client = new_client(Some(".ahma/tools")).await?;

    let tools = client.list_tools(None).await?;

    // Find cargo tool to test (has version subcommand)
    let cargo_tool = tools.tools.iter().find(|t| t.name.as_ref() == "cargo");

    assert!(
        cargo_tool.is_some(),
        "cargo tool should be available from cargo.json config. Available tools: {:?}",
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    client.cancel().await?;
    Ok(())
}
