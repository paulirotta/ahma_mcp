//! Integration tests for CLI parsing functionality
//!
//! These tests verify that ahma_mcp can correctly parse CLI help output
//! and extract command structures from real tools.

use ahma_mcp::cli_parser::{CliOption, CliSubcommand};
use ahma_mcp::{CliParser, CliStructure};
use anyhow::Result;

mod common;
use common::test_utils::contains_any;

/// Test that CLI parser can parse git help output
#[tokio::test]
async fn test_parse_git_help() -> Result<()> {
    let parser = CliParser::new()?;

    // Get git help output
    match parser.get_help_output("git") {
        Ok(help_output) => {
            assert!(
                !help_output.is_empty(),
                "Git help output should not be empty"
            );
            assert!(
                help_output.contains("usage:"),
                "Should contain usage information"
            );

            // Try to parse the help output
            let structure = parser.parse_help_output("git", &help_output)?;

            // Verify the structure was parsed
            assert_eq!(structure.tool_name, "git");
            assert!(
                !structure.global_options.is_empty() || !structure.subcommands.is_empty(),
                "Should have found options or subcommands"
            );
        }
        Err(e) => {
            // If git is not available, skip this test
            let error_msg = e.to_string();
            if error_msg.contains("No such file") || error_msg.contains("not found") {
                println!("Skipping git test - git not available: {}", error_msg);
                return Ok(());
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Test that CLI parser can parse curl help output  
#[tokio::test]
async fn test_parse_curl_help() -> Result<()> {
    let parser = CliParser::new()?;

    // Get curl help output
    match parser.get_help_output("curl") {
        Ok(help_output) => {
            assert!(
                !help_output.is_empty(),
                "Curl help output should not be empty"
            );

            // Try to parse the help output
            let structure = parser.parse_help_output("curl", &help_output)?;

            // Verify the structure was parsed
            assert_eq!(structure.tool_name, "curl");
            // Curl should have many options
            assert!(
                !structure.global_options.is_empty(),
                "Curl should have options"
            );
        }
        Err(e) => {
            // If curl is not available, skip this test
            let error_msg = e.to_string();
            if error_msg.contains("No such file") || error_msg.contains("not found") {
                println!("Skipping curl test - curl not available: {}", error_msg);
                return Ok(());
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Test CLI structure creation and manipulation
#[tokio::test]
async fn test_cli_structure_operations() -> Result<()> {
    let mut structure = CliStructure::new("test_tool".to_string());

    // Test basic structure properties
    assert_eq!(structure.tool_name, "test_tool");
    assert!(structure.global_options.is_empty());
    assert!(structure.subcommands.is_empty());

    // Add some options - we need to manually add since there's no add_option method
    structure.global_options.push(CliOption {
        short: Some('v'),
        long: Some("verbose".to_string()),
        description: "Enable verbose output".to_string(),
        takes_value: false,
        multiple: false,
    });
    structure.global_options.push(CliOption {
        short: Some('h'),
        long: Some("help".to_string()),
        description: "Show help".to_string(),
        takes_value: false,
        multiple: false,
    });

    // Add some subcommands - we need to manually add since there's no add_subcommand method
    structure.subcommands.push(CliSubcommand {
        name: "build".to_string(),
        description: "Build the project".to_string(),
        options: Vec::new(),
    });
    structure.subcommands.push(CliSubcommand {
        name: "test".to_string(),
        description: "Run tests".to_string(),
        options: Vec::new(),
    });

    // Verify additions
    assert_eq!(structure.global_options.len(), 2);
    assert_eq!(structure.subcommands.len(), 2);

    // Test finding options and subcommands
    assert!(
        structure
            .global_options
            .iter()
            .any(|opt| opt.long == Some("verbose".to_string()))
    );
    assert!(structure.subcommands.iter().any(|sub| sub.name == "build"));

    Ok(())
}

/// Test parsing of complex CLI help with multiple sections
#[tokio::test]
async fn test_parse_complex_help_structure() -> Result<()> {
    let parser = CliParser::new()?;

    // Create mock help output that resembles a real CLI tool
    let complex_help = r#"
Usage: mytool [GLOBAL-OPTIONS] <subcommand> [ARGS]

Global Options:
  -v, --verbose     Enable verbose output
  -h, --help        Show this help message
  -V, --version     Show version information
  --config <FILE>   Use custom config file

Commands:
   build    Build the project
   test     Run test suite  
   clean    Clean build artifacts
   deploy   Deploy to production

For more information on a specific subcommand, try 'mytool <subcommand> --help'
"#;

    let structure = parser.parse_help_output("mytool", complex_help)?;

    // Verify the tool name
    assert_eq!(structure.tool_name, "mytool");

    // Should have found multiple options
    assert!(
        !structure.global_options.is_empty(),
        "Should have parsed options"
    );
    assert_eq!(
        structure.global_options.len(),
        4,
        "Should have parsed 4 options"
    );

    // Should have found multiple subcommands
    assert!(
        !structure.subcommands.is_empty(),
        "Should have parsed subcommands"
    );
    assert_eq!(
        structure.subcommands.len(),
        4,
        "Should have parsed 4 subcommands"
    );

    // Check for specific options
    let has_verbose = structure.global_options.iter().any(|opt| {
        opt.long.as_ref().map_or(false, |l| l.contains("verbose")) || opt.short == Some('v')
    });
    assert!(has_verbose, "Should have found verbose option");

    // Check for specific subcommands
    let has_build = structure.subcommands.iter().any(|sub| sub.name == "build");
    assert!(has_build, "Should have found build subcommand");

    let has_test = structure.subcommands.iter().any(|sub| sub.name == "test");
    assert!(has_test, "Should have found test subcommand");

    Ok(())
}

/// Test error handling for invalid commands
#[tokio::test]
async fn test_invalid_command_handling() -> Result<()> {
    let parser = CliParser::new()?;

    // Try to get help for a command that doesn't exist
    let result = parser.get_help_output("this_command_definitely_does_not_exist_anywhere");

    // Should return an error, not crash
    assert!(
        result.is_err(),
        "Should return error for non-existent command"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(contains_any(
        &error_msg,
        &["No such file", "not found", "Failed to execute"]
    ));

    Ok(())
}

/// Test parsing empty or minimal help output
#[tokio::test]
async fn test_parse_minimal_help() -> Result<()> {
    let parser = CliParser::new()?;

    // Test with minimal help output
    let minimal_help = "Usage: simple_tool [options]";
    let structure = parser.parse_help_output("simple_tool", minimal_help)?;

    assert_eq!(structure.tool_name, "simple_tool");
    // With minimal output, might not find specific options but shouldn't crash

    // Test with empty help output
    let empty_structure = parser.parse_help_output("empty_tool", "")?;
    assert_eq!(empty_structure.tool_name, "empty_tool");

    Ok(())
}
