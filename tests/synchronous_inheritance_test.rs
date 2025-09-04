use ahma_mcp::config::ToolConfig;
use anyhow::Result;

#[test]
fn test_synchronous_inheritance_loading() -> Result<()> {
    // Test that tool-level synchronous field is properly loaded
    let json_config = r#"
    {
        "name": "test_tool",
        "description": "Test tool for synchronous inheritance",
        "command": "test",
        "synchronous": true,
        "subcommand": [
            {
                "name": "sub1",
                "description": "Subcommand with no synchronous field",
                "options": []
            },
            {
                "name": "sub2", 
                "description": "Subcommand with explicit synchronous override",
                "synchronous": false,
                "options": []
            }
        ]
    }
    "#;

    let config: ToolConfig = serde_json::from_str(json_config)?;

    // Tool-level synchronous should be loaded
    assert_eq!(config.synchronous, Some(true));

    let subcommands = config.subcommand.as_ref().expect("Should have subcommands");

    // First subcommand should have None (will inherit tool-level)
    assert_eq!(subcommands[0].synchronous, None);

    // Second subcommand should have explicit override
    assert_eq!(subcommands[1].synchronous, Some(false));

    Ok(())
}

#[test]
fn test_synchronous_inheritance_logic() {
    // Test the inheritance logic that would be in mcp_service.rs

    // Test case 1: Subcommand has explicit value, should use it
    let subcommand_sync = Some(false);
    let tool_sync = Some(true);
    let result = subcommand_sync.or(tool_sync).unwrap_or(false);
    assert!(!result, "Should use subcommand value when present");

    // Test case 2: Subcommand has None, should inherit tool value
    let subcommand_sync = None;
    let tool_sync = Some(true);
    let result = subcommand_sync.or(tool_sync).unwrap_or(false);
    assert!(result, "Should inherit tool value when subcommand is None");

    // Test case 3: Both None, should default to false
    let subcommand_sync = None;
    let tool_sync = None;
    let result = subcommand_sync.or(tool_sync).unwrap_or(false);
    assert!(!result, "Should default to false when both are None");
}

#[test]
fn test_gh_tool_optimized_format() -> Result<()> {
    // Test that our optimized gh.json loads correctly
    let gh_json = std::fs::read_to_string("tools/gh.json")?;
    let config: ToolConfig = serde_json::from_str(&gh_json)?;

    // Should NOT have tool-level synchronous field (async is default, no redundant declaration)
    assert_eq!(config.synchronous, None);

    // Subcommands should also NOT have redundant synchronous = false for async behavior
    if let Some(subcommands) = &config.subcommand {
        for subcommand in subcommands {
            assert_eq!(
                subcommand.synchronous, None,
                "Subcommand '{}' should not have redundant synchronous=false (async is default)",
                subcommand.name
            );
        }
    }

    Ok(())
}
