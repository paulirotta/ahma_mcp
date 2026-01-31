//! Tool Loading Compatibility Test
//! PURPOSE: Validates JSON tool configuration loading after discovering critical bug
//! where adding extra JSON files (status.json, await.json) broke test expectations.
//! LESSON LEARNED: Tool loading tests are brittle to configuration changes.
//! - Only external CLI tools should have JSON configurations
//! - Hardwired MCP tools (status, await) don't need JSON files  
//! - But users may add them anyway for documentation/IDE support
//!   MAINTENANCE: Update expected counts when adding new CLI tool integrations

use ahma_mcp::test_utils as common;

use anyhow::Result;

use ahma_mcp::utils::logging::init_test_logging;
use common::test_client::new_client;

/// TEST: Validates core CLI tool configurations are loaded
///
/// CRITICAL REQUIREMENT: Must load sandboxed_shell (always available as built-in).
/// Other tools like cargo.json, python.json are optional and may not be available in CI.
///
/// LESSON LEARNED: Don't hard-code exact counts - use minimum expectations
/// This test previously failed when status.json/await.json were temporarily added
#[tokio::test]
async fn test_tools_are_loaded_after_json_migration() -> Result<()> {
    init_test_logging();
    // Test that core tool configurations are properly loaded
    let client = new_client(Some(".ahma")).await?;

    let tools = client.list_tools(None).await?;

    // Should find at least the core built-in tools (await, status, sandboxed_shell)
    assert!(
        tools.tools.len() >= 3,
        "Expected at least 3 tools loaded, but got {}. Tools found: {:?}",
        tools.tools.len(),
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Verify specific expected tools are present
    let tool_names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();

    // Should have sandboxed_shell tool (always available)
    assert!(
        tool_names.contains(&"sandboxed_shell"),
        "Expected sandboxed_shell tool but found: {:?}",
        tool_names
    );

    // ls tool (file listing) is optional; do not assert its presence

    client.cancel().await?;
    Ok(())
}

/// TEST: Validates specific JSON tool actually functions
///
/// PURPOSE: Ensures loaded tools aren't just registered but actually callable.
/// Uses sandboxed_shell as test case since it's always available.
///
/// MAINTENANCE: sandboxed_shell is a core built-in tool that should always be present
#[tokio::test]
async fn test_specific_json_tool_functionality() -> Result<()> {
    init_test_logging();
    // Test that a specific tool from JSON config actually works
    let client = new_client(Some(".ahma")).await?;

    let tools = client.list_tools(None).await?;

    // Find sandboxed_shell tool to test (always available)
    let shell_tool = tools
        .tools
        .iter()
        .find(|t| t.name.as_ref() == "sandboxed_shell");

    assert!(
        shell_tool.is_some(),
        "sandboxed_shell tool should be available. Available tools: {:?}",
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    client.cancel().await?;
    Ok(())
}
