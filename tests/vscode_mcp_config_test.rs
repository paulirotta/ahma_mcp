//! Test VS Code MCP configuration for JSON tool definitions
use anyhow::Result;
use serde_json::Value;
use std::fs;

#[tokio::test]
async fn test_vscode_mcp_config_watches_json_files() -> Result<()> {
    // Read the VS Code MCP configuration
    let mcp_config_content = fs::read_to_string(".vscode/mcp.json")?;
    let mcp_config: Value = serde_json::from_str(&mcp_config_content)?;

    // Extract the watch patterns from the dev configuration
    let watch_patterns = mcp_config["servers"]["ahma_mcp"]["dev"]["watch"]
        .as_array()
        .expect("Watch patterns should be an array");

    // Check that we're watching JSON files, not TOML files
    let has_json_pattern = watch_patterns
        .iter()
        .any(|pattern| pattern.as_str().unwrap_or("").contains("tools/*.json"));

    let has_toml_pattern = watch_patterns
        .iter()
        .any(|pattern| pattern.as_str().unwrap_or("").contains("tools/*.toml"));

    // This should pass: we should be watching JSON files
    assert!(
        has_json_pattern,
        "VS Code MCP config should watch tools/*.json files"
    );

    // This should pass: we should NOT be watching TOML files
    assert!(
        !has_toml_pattern,
        "VS Code MCP config should not watch obsolete tools/*.toml files"
    );

    Ok(())
}

#[tokio::test]
async fn test_vscode_mcp_config_has_valid_command_structure() -> Result<()> {
    // Read the VS Code MCP configuration
    let mcp_config_content = fs::read_to_string(".vscode/mcp.json")?;
    let mcp_config: Value = serde_json::from_str(&mcp_config_content)?;

    let server_config = &mcp_config["servers"]["ahma_mcp"];

    // Verify basic structure
    assert_eq!(
        server_config["type"], "stdio",
        "Should use stdio communication"
    );
    assert_eq!(
        server_config["cwd"], "${workspaceFolder}",
        "Should set working directory"
    );
    assert_eq!(
        server_config["command"], "cargo",
        "Should use cargo command"
    );

    // Verify args structure for running the server
    let args = server_config["args"]
        .as_array()
        .expect("Args should be an array");

    // Should have the correct cargo run structure
    assert_eq!(args[0], "run", "First arg should be 'run'");
    assert_eq!(args[1], "--release", "Should use release build");
    assert_eq!(args[2], "--bin", "Should specify bin target");
    assert_eq!(args[3], "ahma_mcp", "Should specify ahma_mcp binary");
    assert_eq!(args[4], "--", "Should separate cargo args from binary args");
    assert_eq!(args[5], "--server", "Should run in server mode");
    assert_eq!(args[6], "--tools-dir", "Should specify tools-dir");
    assert_eq!(args[7], "tools", "Should use tools directory");

    Ok(())
}
