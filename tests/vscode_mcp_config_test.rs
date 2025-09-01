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
    let has_json_pattern = watch_patterns.iter().any(|pattern| {
        pattern.as_str().unwrap_or("").contains("tools/*.json")
    });
    
    let has_toml_pattern = watch_patterns.iter().any(|pattern| {
        pattern.as_str().unwrap_or("").contains("tools/*.toml")
    });
    
    // This should pass: we should be watching JSON files
    assert!(has_json_pattern, "VS Code MCP config should watch tools/*.json files");
    
    // This should pass: we should NOT be watching TOML files
    assert!(!has_toml_pattern, "VS Code MCP config should not watch obsolete tools/*.toml files");
    
    Ok(())
}
