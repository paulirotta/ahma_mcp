//! Test for loading tool definitions from JSON files.

mod common;

use anyhow::Result;
use common::test_client::new_client;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_json_tool_definition_loading() -> Result<()> {
    // Create a temporary directory for our test tool definitions.
    let temp_dir = tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    fs::create_dir(&tools_dir)?;

    // Create a sample tool definition in JSON format.
    let json_tool_def = r#"{
        "name": "json_tool",
        "description": "A test tool loaded from a JSON file.",
        "command": "echo",
        "subcommand": [
            {
                "name": "default",
                "description": "A test subcommand.",
                "options": [],
                "synchronous": true
            }
        ],
        "timeout_seconds": 5,
        "enabled": true
    }"#;
    fs::write(tools_dir.join("test_tool.json"), json_tool_def)?;

    // Start the server, pointing it to our temporary tools directory.
    let client = new_client(Some(tools_dir.to_str().unwrap())).await?;

    // List the available tools.
    let tools = client.list_all_tools().await?;
    let tool_names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();

    // Assert that our JSON-defined tool is loaded.
    assert!(
        tool_names.contains(&"json_tool"),
        "The tool 'json_tool' was not found in the loaded tools: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}
