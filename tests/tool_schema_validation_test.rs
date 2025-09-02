//! Validate all tool JSON files in /tools against the schema validator
use ahma_mcp::schema_validation::SchemaValidator;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn test_all_tool_json_files_validate() -> Result<()> {
    let validator = SchemaValidator::new();
    let tools_dir = PathBuf::from("tools");

    // Canonicalize the tools directory to get absolute path for security validation
    let canonical_tools_dir = tools_dir.canonicalize()?;

    let mut had_errors = false;
    let mut reports = Vec::new();

    for entry in fs::read_dir(&tools_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Security: Validate that the path is actually within the tools directory
        // This prevents path traversal attacks
        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue, // Skip files that can't be canonicalized (e.g., broken symlinks)
        };

        if !canonical_path.starts_with(&canonical_tools_dir) {
            continue; // Skip files outside the tools directory
        }

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let contents = fs::read_to_string(&path)?;
        match validator.validate_tool_config(&path, &contents) {
            Ok(_) => {
                // valid
            }
            Err(errors) => {
                had_errors = true;
                let report = validator.format_errors(&errors, &path);
                reports.push(report);
            }
        }
    }

    if had_errors {
        // Join reports to show all issues in a single failure message
        let full = reports.join("\n\n");
        panic!("Tool schema validation failed:\n{}", full);
    }

    Ok(())
}
