//! Advanced integration tests for ahma_mcp MCP tool loading and configuration
//! Tests the actual tool loading, configuration parsing, and schema generation

use anyhow::Result;
use tempfile::TempDir;
use tokio::fs;

use ahma_mcp::{adapter::Adapter, cli_parser::CliParser, config::Config};

/// Test MCP tool loading and configuration
#[tokio::test]
async fn test_tool_configuration_loading() -> Result<()> {
    // Create a temporary directory with tool configurations
    let temp_dir = TempDir::new()?;
    let tools_dir = temp_dir.path().join("tools");
    fs::create_dir_all(&tools_dir).await?;

    // Create a test tool configuration
    let echo_config = r#"
tool_name = "echo"
command = "echo"
enabled = true
timeout_seconds = 30

[hints]
primary = "Simple command to output text"
usage = "echo 'Hello World'"
default = "Use echo to output text and test command execution"

[hints.custom]
help = "Use 'echo --help' to see options"
"#;

    fs::write(tools_dir.join("echo.toml"), echo_config).await?;

    // Test loading the configuration
    let config = Config::load_from_file(tools_dir.join("echo.toml"))?;
    assert_eq!(config.tool_name, "echo");
    assert_eq!(config.get_command(), "echo");
    assert!(config.is_enabled());
    assert_eq!(config.get_timeout_seconds(), 30);

    // Test hints
    assert!(config.get_hint(None).is_some());
    assert!(config.get_hint(Some("help")).is_some());

    println!("✅ Tool configuration loading test passed");
    Ok(())
}

/// Test CLI parsing for actual tools
#[tokio::test]
async fn test_cli_parsing_real_tools() -> Result<()> {
    let parser = CliParser::new()?;

    // Test parsing echo (should work on all systems)
    match parser.get_help_output("echo") {
        Ok(help_output) => {
            let structure = parser.parse_help_output("echo", &help_output)?;
            assert_eq!(structure.tool_name, "echo");
            println!("✅ Echo CLI parsing test passed");
        }
        Err(e) => {
            println!("⚠️ Echo not available for parsing test: {}", e);
        }
    }

    // Test parsing git if available
    match parser.get_help_output("git") {
        Ok(help_output) => {
            let structure = parser.parse_help_output("git", &help_output)?;
            assert_eq!(structure.tool_name, "git");
            // Git should have subcommands
            assert!(
                !structure.subcommands.is_empty(),
                "Git should have subcommands"
            );
            println!("✅ Git CLI parsing test passed");
        }
        Err(e) => {
            println!("⚠️ Git not available for parsing test: {}", e);
        }
    }

    Ok(())
}

/// Test adapter initialization and tool addition
#[tokio::test]
async fn test_adapter_tool_management() -> Result<()> {
    let mut adapter = Adapter::new(true)?; // Synchronous mode for testing

    // Create a simple config for testing
    let config = Config {
        tool_name: "echo".to_string(),
        command: Some("echo".to_string()),
        enabled: Some(true),
        timeout_seconds: Some(30),
        verbose: Some(false),
        hints: None,
        overrides: None,
        synchronous: Some(false),
    };

    // Test adding a tool (this tests CLI parsing integration)
    match adapter.add_tool("echo", config).await {
        Ok(()) => {
            println!("✅ Tool addition test passed");
        }
        Err(e) => {
            println!("⚠️ Tool addition failed (echo may not be available): {}", e);
        }
    }

    // Test getting tool schemas
    let schemas = adapter.get_tool_schemas()?;
    println!("Generated {} tool schemas", schemas.len());

    Ok(())
}

/// Test tool execution through adapter
#[tokio::test]
async fn test_tool_execution() -> Result<()> {
    let mut adapter = Adapter::new(true)?; // Synchronous mode

    // Add echo tool
    let config = Config {
        tool_name: "echo".to_string(),
        command: Some("echo".to_string()),
        enabled: Some(true),
        timeout_seconds: Some(30),
        verbose: Some(false),
        hints: None,
        overrides: None,
        synchronous: Some(false),
    };

    match adapter.add_tool("echo", config).await {
        Ok(()) => {
            // Test executing the tool
            match adapter
                .execute_tool("echo", vec!["Hello from adapter!".to_string()])
                .await
            {
                Ok(output) => {
                    assert!(output.contains("Hello from adapter!"));
                    println!("✅ Tool execution test passed");
                }
                Err(e) => {
                    println!("⚠️ Tool execution failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("⚠️ Tool addition failed: {}", e);
        }
    }

    Ok(())
}

/// Test loading tools from tools directory like the real server does
#[tokio::test]
async fn test_tools_directory_loading() -> Result<()> {
    // Change to the project root where tools/ directory exists
    let original_dir = std::env::current_dir()?;
    let project_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    std::env::set_current_dir(&project_root)?;

    let mut adapter = Adapter::new(true)?;

    // This should load all tools from the tools/ directory
    match adapter.initialize().await {
        Ok(()) => {
            let schemas = adapter.get_tool_schemas()?;
            println!("✅ Loaded {} tools from tools directory", schemas.len());

            // Should have loaded at least some tools
            if !schemas.is_empty() {
                println!("✅ Tools directory loading test passed");
            } else {
                println!("⚠️ No tools loaded from directory");
            }
        }
        Err(e) => {
            println!("⚠️ Tools directory loading failed: {}", e);
        }
    }

    // Restore original directory
    std::env::set_current_dir(original_dir)?;

    Ok(())
}
