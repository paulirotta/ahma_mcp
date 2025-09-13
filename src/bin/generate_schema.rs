#!/usr/bin/env rust-script
//! Generate MTDF JSON Schema
//!
//! This binary generates the JSON schema for the Multi-Tool Definition Format (MTDF)
//! and writes it to docs/mtdf-schema.json

use ahma_mcp::config::ToolConfig;
use ahma_mcp::utils::logging::init_logging;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging("info", false)?;

    // Generate schema for the root ToolConfig type
    let schema = schemars::schema_for!(ToolConfig);

    // Convert to pretty JSON
    let schema_json = serde_json::to_string_pretty(&schema)?;

    // Allow overriding the output directory via command-line argument
    let output_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("docs"));

    // Ensure docs directory exists
    fs::create_dir_all(&output_dir)?;

    // Write to docs directory
    let docs_path = output_dir.join("mtdf-schema.json");
    fs::write(&docs_path, &schema_json)?;

    println!("✓ Generated MTDF JSON Schema at: {}", docs_path.display());
    println!("  Schema size: {} bytes", schema_json.len());

    // Also show first few lines for verification
    let lines: Vec<&str> = schema_json.lines().take(10).collect();
    println!("  Preview:");
    for line in lines {
        println!("    {}", line);
    }
    if schema_json.lines().count() > 10 {
        println!(
            "    ... and {} more lines",
            schema_json.lines().count() - 10
        );
    }

    Ok(())
}
