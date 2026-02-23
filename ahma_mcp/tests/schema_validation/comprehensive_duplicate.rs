//! Test that MCP `tools/list` never returns duplicate tool names.
//!
//! This covers the integration-level scenario where both hardcoded tools
//! (await, status, sandboxed_shell) and config-driven tools are assembled
//! into a single response. The synthetic `sandboxed_shell` ToolConfig
//! inserted by `load_tool_configs()` must be filtered out by the
//! `HARDCODED_TOOLS` guard in `list_tools()`.

use ahma_mcp::test_utils::client::ClientBuilder;
use std::collections::HashSet;
use std::time::Duration;
use tempfile::TempDir;

/// Verify that `tools/list` response contains no duplicate tool names,
/// even when `.ahma/` directory has multiple tool configs alongside
/// the synthetic sandboxed_shell entry.
#[tokio::test]
async fn test_tools_list_no_duplicate_names() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let cwd = temp_dir.path();

    // Create .ahma directory with two test tools
    let ahma_dir = cwd.join(".ahma");
    std::fs::create_dir(&ahma_dir)?;

    for (file, name, desc) in &[
        ("tool_alpha.json", "tool_alpha", "Alpha tool"),
        ("tool_beta.json", "tool_beta", "Beta tool"),
    ] {
        let json = format!(
            r#"{{
    "name": "{}",
    "description": "{}",
    "command": "echo",
    "enabled": true,
    "subcommand": [{{ "name": "default", "description": "test" }}]
}}"#,
            name, desc
        );
        std::fs::write(ahma_dir.join(file), json)?;
    }

    let service = ClientBuilder::new().working_dir(cwd).build().await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let tools_result = service.list_tools(None).await?;
    let tools = tools_result.tools;

    // Collect all tool names and check for duplicates
    let mut seen = HashSet::new();
    for tool in &tools {
        let name: &str = &tool.name;
        assert!(
            seen.insert(name.to_string()),
            "Duplicate tool name '{}' found in tools/list response. All tools: {:?}",
            tool.name,
            tools.iter().map(|t| t.name.to_string()).collect::<Vec<_>>()
        );
    }

    // Verify exactly one sandboxed_shell
    let shell_count = tools.iter().filter(|t| t.name == "sandboxed_shell").count();
    assert_eq!(
        shell_count,
        1,
        "Expected exactly 1 sandboxed_shell, found {}. All tools: {:?}",
        shell_count,
        tools.iter().map(|t| t.name.to_string()).collect::<Vec<_>>()
    );

    // Verify built-ins are all present
    assert!(tools.iter().any(|t| t.name == "await"));
    assert!(tools.iter().any(|t| t.name == "status"));
    assert!(tools.iter().any(|t| t.name == "sandboxed_shell"));

    // Verify user tools are present
    assert!(tools.iter().any(|t| t.name == "tool_alpha"));
    assert!(tools.iter().any(|t| t.name == "tool_beta"));

    Ok(())
}
