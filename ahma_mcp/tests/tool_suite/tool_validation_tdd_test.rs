use ahma_mcp::test_utils as common;

use std::process::Command;

/// Test-driven development approach to validate tool configurations and usage
#[cfg(test)]
mod tool_validation_tdd_tests {
    use super::*;
    use common::fs::get_workspace_dir;

    #[test]
    fn test_cargo_test_is_available_via_cargo_tool() {
        // TDD: cargo test should be available as a subcommand in the cargo tool
        let workspace_dir = get_workspace_dir();
        let tools_dir = workspace_dir.join(".ahma");
        let tools_dir_str = tools_dir.to_string_lossy().into_owned();
        let output = Command::new("cargo")
            .current_dir(&workspace_dir)
            .arg("run")
            .arg("--package")
            .arg("ahma_validate")
            .arg("--bin")
            .arg("ahma_validate")
            .arg("--")
            .arg(&tools_dir_str)
            .output()
            .expect("Failed to run ahma_validate");

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
        // TDD: All cargo subcommands must live in cargo.json (no separate cargo-X files)
        let workspace_dir = get_workspace_dir();
        let cargo_path = workspace_dir.join(".ahma/cargo.json");
        assert!(
            cargo_path.exists(),
            "cargo.json should exist for cargo subcommands"
        );

        // Validate that cargo.json actually exposes nextest run subcommand
        let cargo_config =
            std::fs::read_to_string(&cargo_path).expect("Failed to read cargo.json after merge");
        let cargo_json: serde_json::Value =
            serde_json::from_str(&cargo_config).expect("cargo.json should remain valid JSON");

        let subcommands = cargo_json["subcommand"]
            .as_array()
            .expect("cargo.json should include subcommand array");

        let has_nextest = subcommands.iter().any(|sub| {
            sub["name"].as_str() == Some("nextest")
                && sub["subcommand"].as_array().is_some_and(|nested| {
                    nested
                        .iter()
                        .any(|child| child["name"].as_str() == Some("run"))
                })
        });

        assert!(has_nextest, "cargo.json must embed nextest run subcommand");

        // llvm-cov has been removed from cargo.json because its instrumentation conflicts with
        // MCP sandboxing. Users should run `cargo llvm-cov` directly in terminal for coverage.
        // CI handles coverage separately in the job-coverage workflow.
        let has_llvm_cov = subcommands
            .iter()
            .any(|sub| sub["name"].as_str() == Some("llvm-cov"));
        assert!(
            !has_llvm_cov,
            "llvm-cov should NOT be in cargo.json (removed due to sandbox incompatibility)"
        );
    }

    #[test]
    fn test_all_json_files_in_tools_directory_are_valid_json() {
        // TDD: Ensure all .json files in tools directory contain valid JSON
        let workspace_dir = get_workspace_dir();
        let tools_dir = workspace_dir.join(".ahma");
        assert!(tools_dir.exists(), "Tools directory should exist");

        for entry in std::fs::read_dir(&tools_dir).expect("Failed to read tools directory") {
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
        let workspace_dir = get_workspace_dir();
        let tools_dir = workspace_dir.join(".ahma");
        assert!(tools_dir.exists(), "Tools directory should exist");

        let mut toml_files = Vec::new();
        for entry in std::fs::read_dir(&tools_dir).expect("Failed to read tools directory") {
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
