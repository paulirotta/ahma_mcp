#!/usr/bin/env rust-script
//! Generate MTDF JSON Schema
//!
//! This binary generates the JSON schema for the Multi-Tool Definition Format (MTDF)
//! and writes it to docs/mtdf-schema.json

use ahma_mcp::config::ToolConfig;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generate schema for the root ToolConfig type
    let schema = schemars::schema_for!(ToolConfig);

    // Convert to pretty JSON
    let schema_json = serde_json::to_string_pretty(&schema)?;

    // Ensure docs directory exists
    fs::create_dir_all("docs")?;

    // Write to docs directory
    let docs_path = Path::new("docs").join("mtdf-schema.json");
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
