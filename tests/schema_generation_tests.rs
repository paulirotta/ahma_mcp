//! Integration tests for MCP schema generation functionality
//!
//! These tests verify that ahma_mcp can correctly generate MCP tool schemas
//! from parsed CLI structures and configurations.

use ahma_mcp::cli_parser::{CliOption, CliStructure, CliSubcommand};
use ahma_mcp::{Adapter, Config, McpSchemaGenerator};
use anyhow::Result;
use serde_json::json;

mod common;
use common::test_project::{TestProjectOptions, create_test_project};

/// Test basic schema generation for a tool without subcommands
#[tokio::test]
async fn test_generate_schema_simple_tool() -> Result<()> {
    let generator = McpSchemaGenerator::new();
    let mut structure = CliStructure::new("echo".to_string());

    // Add some basic options
    structure.global_options.push(CliOption {
        short: Some('n'),
        long: None,
        description: "Do not output trailing newline".to_string(),
        takes_value: false,
        multiple: false,
    });

    structure.global_options.push(CliOption {
        short: None,
        long: Some("help".to_string()),
        description: "Show help message".to_string(),
        takes_value: false,
        multiple: false,
    });

    let config = Config::default();
    let schema = generator.generate_tool_schema(&structure, &config)?;

    // Verify basic schema structure
    assert_eq!(schema["name"], "echo");
    assert!(schema["description"].as_str().unwrap().contains("echo"));

    let input_schema = &schema["inputSchema"];
    assert_eq!(input_schema["type"], "object");

    let properties = &input_schema["properties"];
    assert!(properties.is_object());

    // Should have the 'n' option but not help (filtered out)
    assert!(properties["n"].is_object());
    assert_eq!(properties["n"]["type"], "boolean");

    // Help should be filtered out
    assert!(properties["help"].is_null() || !properties.as_object().unwrap().contains_key("help"));

    // Should have common parameters
    assert!(properties["working_directory"].is_object());
    assert!(properties["enable_async_notification"].is_object());
    assert!(properties["args"].is_object());

    Ok(())
}

/// Test schema generation for a tool with subcommands
#[tokio::test]
async fn test_generate_schema_with_subcommands() -> Result<()> {
    let generator = McpSchemaGenerator::new();
    let mut structure = CliStructure::new("git".to_string());

    // Add global options
    structure.global_options.push(CliOption {
        short: Some('v'),
        long: Some("verbose".to_string()),
        description: "Enable verbose output".to_string(),
        takes_value: false,
        multiple: false,
    });

    structure.global_options.push(CliOption {
        short: Some('C'),
        long: None,
        description: "Run as if git was started in <path>".to_string(),
        takes_value: true,
        multiple: false,
    });

    // Add subcommands
    structure.subcommands.push(CliSubcommand {
        name: "add".to_string(),
        description: "Add file contents to the index".to_string(),
        options: Vec::new(),
    });

    structure.subcommands.push(CliSubcommand {
        name: "commit".to_string(),
        description: "Record changes to the repository".to_string(),
        options: Vec::new(),
    });

    let config = Config::default();
    let schema = generator.generate_tool_schema(&structure, &config)?;

    // Verify basic schema structure
    assert_eq!(schema["name"], "git");
    assert!(schema["description"].as_str().unwrap().contains("git"));

    let input_schema = &schema["inputSchema"];
    let properties = &input_schema["properties"];
    let required = &input_schema["required"];

    // Should have subcommand parameter
    assert!(properties["subcommand"].is_object());
    assert!(required.as_array().unwrap().contains(&json!("subcommand")));

    // Should have enum with subcommand names
    let subcommand_enum = properties["subcommand"]["enum"].as_array().unwrap();
    assert!(subcommand_enum.contains(&json!("add")));
    assert!(subcommand_enum.contains(&json!("commit")));

    // Should have global options
    assert!(properties["verbose"].is_object());
    assert_eq!(properties["verbose"]["type"], "boolean");
    assert!(properties["C"].is_object());
    assert_eq!(properties["C"]["type"], "string");

    // Should have common parameters
    assert!(properties["working_directory"].is_object());
    assert!(properties["enable_async_notification"].is_object());
    assert!(properties["args"].is_object());

    Ok(())
}

/// Test tools manifest generation
#[tokio::test]
async fn test_generate_tools_manifest() -> Result<()> {
    let generator = McpSchemaGenerator::new();

    // Create multiple tool structures
    let mut echo_structure = CliStructure::new("echo".to_string());
    echo_structure.global_options.push(CliOption {
        short: Some('n'),
        long: None,
        description: "Do not output trailing newline".to_string(),
        takes_value: false,
        multiple: false,
    });

    let mut git_structure = CliStructure::new("git".to_string());
    git_structure.subcommands.push(CliSubcommand {
        name: "add".to_string(),
        description: "Add file contents to the index".to_string(),
        options: Vec::new(),
    });

    // Create tool configurations
    let echo_config = Config::default();
    let git_config = Config::default();

    let tools = vec![(echo_structure, echo_config), (git_structure, git_config)];

    let manifest = generator.generate_tools_manifest(&tools)?;

    // Should have a "tools" field with array of 2 tools
    assert!(manifest["tools"].is_array());
    let tools_array = manifest["tools"].as_array().unwrap();
    assert_eq!(tools_array.len(), 2);

    // Check tool names
    let tool_names: Vec<&str> = tools_array
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"echo"));
    assert!(tool_names.contains(&"git"));

    Ok(())
}

/// Test schema generation with custom configuration
#[tokio::test]
async fn test_generate_schema_with_config() -> Result<()> {
    let _test_project = create_test_project(TestProjectOptions::default()).await?;

    let generator = McpSchemaGenerator::new();
    let mut structure = CliStructure::new("test_tool".to_string());

    structure.global_options.push(CliOption {
        short: Some('v'),
        long: Some("verbose".to_string()),
        description: "Enable verbose output".to_string(),
        takes_value: false,
        multiple: false,
    });

    // Create config with custom settings
    let config = Config::default();

    let schema = generator.generate_tool_schema(&structure, &config)?;

    // Should use tool description from schema
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("test_tool")
    );

    Ok(())
}

/// Test end-to-end schema generation via Adapter
#[tokio::test]
async fn test_adapter_schema_generation() -> Result<()> {
    let test_project = create_test_project(TestProjectOptions {
        with_tool_configs: true,
        ..Default::default()
    })
    .await?;

    // Create tool configurations files manually
    let tools_dir = test_project.path().join("tools");
    tokio::fs::create_dir_all(&tools_dir).await?;

    let curl_config = r#"
tool_name = "curl"
enabled = true
command = "curl"
timeout_seconds = 30
"#;
    tokio::fs::write(tools_dir.join("curl.toml"), curl_config).await?;

    let git_config = r#"
tool_name = "git" 
enabled = true
command = "git"
timeout_seconds = 60
"#;
    tokio::fs::write(tools_dir.join("git.toml"), git_config).await?;

    // Initialize adapter
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(test_project.path())?;
    let mut adapter = Adapter::new(false)?;
    let result = adapter.initialize().await;
    std::env::set_current_dir(original_dir)?;

    result?;

    // Generate schemas
    let schemas = adapter.get_tool_schemas()?;

    // Should have schemas for enabled tools
    assert!(!schemas.is_empty());

    // Check that schemas have correct structure
    for schema in &schemas {
        assert!(schema["name"].is_string());
        assert!(schema["description"].is_string());
        assert!(schema["inputSchema"].is_object());

        let input_schema = &schema["inputSchema"];
        assert_eq!(input_schema["type"], "object");
        assert!(input_schema["properties"].is_object());
    }

    Ok(())
}

/// Test schema generation error handling
#[tokio::test]
async fn test_schema_generation_error_handling() -> Result<()> {
    let generator = McpSchemaGenerator::new();

    // Create a structure with invalid configuration
    let structure = CliStructure::new("".to_string()); // Empty tool name
    let config = Config::default();

    // Should handle gracefully
    let result = generator.generate_tool_schema(&structure, &config);
    // The exact error handling depends on implementation - this tests it doesn't panic

    match result {
        Ok(_) => {
            // If it succeeds, that's fine too - just ensure it has some reasonable defaults
        }
        Err(_) => {
            // If it errors, that's expected for invalid input
        }
    }

    Ok(())
}
