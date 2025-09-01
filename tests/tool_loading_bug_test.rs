//! Test to reproduce the "0 tools found" bug after JSON migration
use anyhow::Result;

mod common;
use common::test_client::new_client;

#[tokio::test]
async fn test_tools_are_loaded_after_json_migration() -> Result<()> {
    // Test that all 3 JSON tool configurations are properly loaded
    let client = new_client(Some("tools")).await?;

    let tools = client.list_tools(None).await?;
    
    // Should find tools from cargo.json, ls.json, python3.json
    assert!(
        tools.tools.len() >= 3,
        "Expected at least 3 tools loaded from JSON configs, but got {}. Tools found: {:?}",
        tools.tools.len(),
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Verify specific expected tools are present
    let tool_names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    
    // Should have cargo subcommand tools
    assert!(
        tool_names.iter().any(|name| name.starts_with("cargo_")),
        "Expected cargo subcommand tools but found: {:?}",
        tool_names
    );
    
    // Should have ls subcommand tools  
    assert!(
        tool_names.iter().any(|name| name.starts_with("ls_")),
        "Expected ls subcommand tools but found: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test] 
async fn test_specific_json_tool_functionality() -> Result<()> {
    // Test that a specific tool from JSON config actually works
    let client = new_client(Some("tools")).await?;
    
    let tools = client.list_tools(None).await?;
    
    // Find a synchronous tool to test (like cargo_version)
    let version_tool = tools.tools.iter().find(|t| t.name.as_ref() == "cargo_version");
    
    assert!(
        version_tool.is_some(),
        "cargo_version tool should be available from cargo.json config. Available tools: {:?}",
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    client.cancel().await?;
    Ok(())
}
