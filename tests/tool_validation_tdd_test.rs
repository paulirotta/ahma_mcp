use std::process::Command;

/// Test-driven development approach to validate tool configurations and usage
#[cfg(test)]
mod tool_validation_tdd_tests {
    use super::*;

    #[test]
    fn test_cargo_test_is_available_via_cargo_tool() {
        // TDD: cargo test should be available as a subcommand in the cargo tool
        let output = Command::new("cargo")
            .args([
                "run",
                "--bin",
                "ahma_mcp",
                "--",
                "--tools-dir",
                "tools",
                "--validate",
                "tools",
            ])
            .output()
            .expect("Failed to run ahma_mcp validation");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        println!("STDOUT: {}", stdout);
        println!("STDERR: {}", stderr);

        // Should be able to validate without errors
        assert!(output.status.success(), "Validation should succeed");

        // Should include cargo tool which contains test subcommand
        assert!(
            stdout.contains("cargo.json") || stderr.contains("cargo.json"),
            "cargo.json tool should be available in ahma_mcp"
        );
    }

    #[test]
    fn test_mcp_ahma_mcp_tools_should_include_cargo_commands() {
        // TDD: The MCP ahma_mcp service should provide cargo_test and cargo_nextest tools
        // This would normally be tested by checking available MCP tools, but for now
        // we verify the tool files exist

        assert!(
            std::path::Path::new("tools/cargo.json").exists(),
            "cargo.json should exist for cargo subcommands"
        );
        assert!(
            std::path::Path::new("tools/cargo_nextest.json").exists(),
            "cargo_nextest.json should exist for nextest commands"
        );
    }

    #[test]
    fn test_all_json_files_in_tools_directory_are_valid_json() {
        // TDD: Ensure all .json files in tools directory contain valid JSON
        let tools_dir = std::path::Path::new("tools");
        assert!(tools_dir.exists(), "Tools directory should exist");

        for entry in std::fs::read_dir(tools_dir).expect("Failed to read tools directory") {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "json") {
                let content = std::fs::read_to_string(&path)
                    .unwrap_or_else(|_| panic!("Failed to read file: {:?}", path));

                let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&content);
                assert!(
                    parse_result.is_ok(),
                    "File {:?} should contain valid JSON. Error: {:?}",
                    path,
                    parse_result.err()
                );
            }
        }
    }

    #[test]
    fn test_no_toml_files_exist_in_tools_directory() {
        // TDD: Ensure there are no .toml files causing formatting errors
        let tools_dir = std::path::Path::new("tools");
        assert!(tools_dir.exists(), "Tools directory should exist");

        let mut toml_files = Vec::new();
        for entry in std::fs::read_dir(tools_dir).expect("Failed to read tools directory") {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "toml") {
                toml_files.push(path);
            }
        }

        assert!(
            toml_files.is_empty(),
            "No .toml files should exist in tools directory. Found: {:?}",
            toml_files
        );
    }

    #[test]
    fn test_identify_formatting_issue_source() {
        // TDD: VSCode shows 1000+ formatting errors - let's identify the source

        // Check Cargo.toml syntax
        let cargo_toml_content =
            std::fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");

        // Try to parse as TOML
        let toml_parse_result = toml::from_str::<toml::Value>(&cargo_toml_content);
        assert!(
            toml_parse_result.is_ok(),
            "Cargo.toml should be valid TOML. Error: {:?}",
            toml_parse_result.err()
        );

        // Check for hidden .toml files that might contain JSON
        let mut problematic_files = Vec::new();

        for entry in std::fs::read_dir(".").expect("Failed to read current directory") {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            // Security: Validate path is safe and within current directory
            if !path.starts_with(".") {
                continue;
            }

            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".toml") && name_str != "Cargo.toml" {
                    // Security: Additional validation - ensure path is a regular file
                    if !path.is_file() {
                        continue;
                    }

                    // Security: Canonicalize path to prevent traversal attacks
                    let canonical_path = match path.canonicalize() {
                        Ok(p) => p,
                        Err(_) => continue, // Skip if path cannot be canonicalized
                    };

                    // Security: Ensure canonical path is still within current directory
                    let current_dir =
                        std::env::current_dir().expect("Failed to get current directory");
                    let canonical_current = current_dir
                        .canonicalize()
                        .expect("Failed to canonicalize current directory");

                    if !canonical_path.starts_with(&canonical_current) {
                        continue; // Skip paths outside current directory
                    }

                    let content = std::fs::read_to_string(&canonical_path)
                        .unwrap_or_else(|_| panic!("Failed to read file: {:?}", canonical_path));

                    // Check if it's actually JSON disguised as TOML
                    if content.trim().starts_with('{') && content.trim().ends_with('}') {
                        problematic_files.push(canonical_path.clone());
                    }

                    // Try to parse as TOML
                    let toml_result = toml::from_str::<toml::Value>(&content);
                    if toml_result.is_err() {
                        problematic_files.push(canonical_path.clone());
                    }
                }
            }
        }

        assert!(
            problematic_files.is_empty(),
            "Found problematic .toml files: {:?}",
            problematic_files
        );
    }
}
