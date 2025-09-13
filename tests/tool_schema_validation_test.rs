//! Validate all tool JSON files in /tools against the ToolConfig struct
use ahma_mcp::config::load_tool_configs;
use ahma_mcp::utils::logging::init_test_logging;
use anyhow::Result;
use std::path::Path;

#[tokio::test]
async fn test_all_tool_json_files_load_correctly() -> Result<()> {
    init_test_logging();
    let tools_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".ahma/tools");
    let tool_configs = load_tool_configs(&tools_dir);

    assert!(
        tool_configs.is_ok(),
        "Failed to load tool configs: {:#?}",
        tool_configs.err().unwrap()
    );

    let tool_configs = tool_configs.unwrap();
    assert!(!tool_configs.is_empty(), "No tool configs were loaded");

    println!(
        "Successfully loaded and validated {} tools:",
        tool_configs.len()
    );
    for (name, config) in tool_configs {
        println!("  - {}: {}", name, config.description);
    }

    Ok(())
}
