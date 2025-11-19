//! Test VS Code MCP configuration for JSON tool definitions
use ahma_core::test_utils as common;

use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use common::get_workspace_path;
use serde_json::Value;
use std::fs;

#[tokio::test]
async fn test_vscode_mcp_config_watches_binary_only() -> Result<()> {
    init_test_logging();
    // Read the VS Code MCP configuration
    let mcp_config_path = get_workspace_path(".vscode/mcp.json");
    let mcp_config_content = fs::read_to_string(mcp_config_path)?;
    let mcp_config: Value = serde_json::from_str(&mcp_config_content)?;

    // Extract the watch patterns from the dev configuration
    let watch_patterns = mcp_config["servers"]["ahma_mcp"]["dev"]["watch"]
        .as_array()
        .expect("Watch patterns should be an array");

    // Check that we're only watching the binary, not tool config files
    let has_binary_pattern = watch_patterns.iter().any(|pattern| {
        pattern
            .as_str()
            .unwrap_or("")
            .contains("target/release/ahma_mcp")
    });

    let has_json_pattern = watch_patterns
        .iter()
        .any(|pattern| pattern.as_str().unwrap_or("").contains("tools/*.json"));

    let has_toml_pattern = watch_patterns
        .iter()
        .any(|pattern| pattern.as_str().unwrap_or("").contains("tools/*.toml"));

    // This should pass: we should be watching the binary
    assert!(
        has_binary_pattern,
        "VS Code MCP config should watch the binary target/release/ahma_mcp"
    );

    // This should pass: we should NOT be watching JSON files (causes too many restarts)
    assert!(
        !has_json_pattern,
        "VS Code MCP config should not watch tools/*.json files (causes restart issues)"
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
    init_test_logging();
    // Read the VS Code MCP configuration
    let mcp_config_path = get_workspace_path(".vscode/mcp.json");
    let mcp_config_content = fs::read_to_string(mcp_config_path)?;
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
        server_config["command"], "target/release/ahma_mcp",
        "Default command should run the release ahma_mcp binary"
    );

    // Verify default args structure for running the binary directly
    let args = server_config["args"]
        .as_array()
        .expect("Args should be an array");
    assert_eq!(args.len(), 2, "Should only include tools-dir arguments");
    assert_eq!(args[0], "--tools-dir", "Should specify tools-dir");
    assert_eq!(args[1], ".ahma/tools", "Should use tools directory");

    // Verify dev command uses cargo with passthrough args
    let dev = &server_config["dev"];
    assert_eq!(dev["command"], "cargo", "Dev command should use cargo");
    let dev_args = dev["args"].as_array().expect("Dev args should be an array");
    assert_eq!(dev_args[0], "run", "Dev first arg should be 'run'");
    assert_eq!(dev_args[1], "--release", "Dev should use release build");
    assert_eq!(
        dev_args[2], "--",
        "Dev should separate cargo args from binary args"
    );
    assert_eq!(dev_args.len(), 5, "Dev args should include tools-dir only");
    assert_eq!(dev_args[3], "--tools-dir", "Dev should specify tools-dir");
    assert_eq!(dev_args[4], ".ahma/tools", "Dev should use tools directory");

    Ok(())
}
