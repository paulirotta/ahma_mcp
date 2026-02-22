//! Python Tool Configuration Example
//!
//! This example demonstrates how to load and validate the python tool configuration
//! from the examples/configs directory.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example python_tool
//! ```

use ahma_mcp::schema_validation::MtdfValidator;
use std::path::PathBuf;
use std::process;

fn main() {
    // Get the workspace root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("examples/configs/python.json");

    println!(
        "Loading python tool configuration from: {}",
        config_path.display()
    );

    // Read the configuration file
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("FAIL Failed to read config file: {}", e);
            process::exit(1);
        }
    };

    // Create validator and validate the configuration
    let validator = MtdfValidator::new();
    match validator.validate_tool_config(&config_path, &content) {
        Ok(config) => {
            println!("OK Configuration is valid!");
            println!("\n📋 Tool Details:");
            println!("   Name: {}", config.name);
            println!("   Description: {}", config.description);
            println!("   Command: {}", config.command);
            println!("   Enabled: {}", config.enabled);
            let subcommands = config.subcommand.as_ref().map(|s| s.len()).unwrap_or(0);
            println!("   Subcommands: {}", subcommands);

            println!("\n🔧 Available Subcommands:");
            if let Some(subcommands) = &config.subcommand {
                for subcommand in subcommands {
                    println!("   - {}: {}", subcommand.name, subcommand.description);
                }
            }
        }
        Err(errors) => {
            eprintln!("FAIL Validation failed with {} error(s):", errors.len());
            for error in errors {
                eprintln!("   - {}: {}", error.field_path, error.message);
                if let Some(suggestion) = error.suggestion {
                    eprintln!("     Suggestion Suggestion: {}", suggestion);
                }
            }
            process::exit(1);
        }
    }
}
