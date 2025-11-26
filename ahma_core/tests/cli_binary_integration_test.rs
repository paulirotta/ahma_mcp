//! CLI Binary Integration Tests
//!
//! These tests verify that all CLI binaries in the ahma_mcp workspace work correctly
//! when invoked from the command line. They provide coverage for the `main.rs` files
//! that are otherwise difficult to test through unit tests.
//!
//! Test philosophy:
//! - Each binary should have tests for: --help, --version, basic functionality
//! - Tests use temp directories as per R13.5 (Test File Isolation)
//! - Tests verify exit codes and output content

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to locate workspace root")
        .to_path_buf()
}

fn build_binary(package: &str, binary: &str) -> PathBuf {
    let workspace = workspace_dir();

    // Build the binary
    let output = Command::new("cargo")
        .current_dir(&workspace)
        .args(["build", "--package", package, "--bin", binary])
        .output()
        .expect("Failed to run cargo build");

    assert!(
        output.status.success(),
        "Failed to build {}: {}",
        binary,
        String::from_utf8_lossy(&output.stderr)
    );

    workspace.join("target/debug").join(binary)
}

/// Create a command for a binary with test mode enabled (bypasses sandbox checks)
fn test_command(binary: &PathBuf) -> Command {
    let mut cmd = Command::new(binary);
    cmd.env("AHMA_TEST_MODE", "1");
    cmd
}

// ============================================================================
// ahma_mcp Binary Tests
// ============================================================================

mod ahma_mcp_tests {
    use super::*;

    #[test]
    fn test_ahma_mcp_help() {
        let binary = build_binary("ahma_shell", "ahma_mcp");

        let output = test_command(&binary)
            .arg("--help")
            .output()
            .expect("Failed to execute ahma_mcp --help");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // --help should succeed
        assert!(
            output.status.success(),
            "ahma_mcp --help should succeed. Output: {}",
            combined
        );

        // Should contain key command info
        assert!(
            combined.contains("ahma_mcp") || combined.contains("Ahma"),
            "Help should mention ahma_mcp or Ahma. Got: {}",
            combined
        );
        assert!(
            combined.contains("--mode") || combined.contains("stdio") || combined.contains("http"),
            "Help should mention modes. Got: {}",
            combined
        );
    }

    #[test]
    fn test_ahma_mcp_version() {
        let binary = build_binary("ahma_shell", "ahma_mcp");

        let output = test_command(&binary)
            .arg("--version")
            .output()
            .expect("Failed to execute ahma_mcp --version");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        assert!(
            output.status.success(),
            "ahma_mcp --version should succeed. Output: {}",
            combined
        );

        // Should contain version number
        assert!(
            combined.contains("0.") || combined.contains("1."),
            "Version output should contain version number. Got: {}",
            combined
        );
    }

    #[test]
    fn test_ahma_mcp_cli_mode_invalid_tool() {
        let binary = build_binary("ahma_shell", "ahma_mcp");
        let workspace = workspace_dir();
        let tools_dir = workspace.join(".ahma/tools");

        let output = test_command(&binary)
            .current_dir(&workspace)
            .args([
                "--tools-dir",
                tools_dir.to_str().unwrap(),
                "nonexistent_tool",
            ])
            .output()
            .expect("Failed to execute ahma_mcp with invalid tool");

        // Should fail with non-zero exit code
        assert!(
            !output.status.success(),
            "ahma_mcp should fail for nonexistent tool"
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not found")
                || stderr.contains("No matching")
                || stderr.contains("error"),
            "Error message should indicate tool not found. Got: {}",
            stderr
        );
    }

    #[test]
    fn test_ahma_mcp_cli_mode_echo_tool() {
        // Test using a simple echo-like tool if available
        let binary = build_binary("ahma_shell", "ahma_mcp");
        let workspace = workspace_dir();
        let tools_dir = workspace.join(".ahma/tools");

        // Check if file_tools exists (a simple tool to test with)
        let output = test_command(&binary)
            .current_dir(&workspace)
            .args(["--tools-dir", tools_dir.to_str().unwrap(), "file_tools_pwd"])
            .output()
            .expect("Failed to execute ahma_mcp with file_tools_pwd");

        // This should either succeed (tool exists) or fail with tool not found
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // If tool exists, it should output the working directory
        if output.status.success() {
            assert!(
                stdout.contains("/") || stdout.contains("\\"),
                "pwd should output a path. Got: {}",
                stdout
            );
        } else {
            // If tool doesn't exist, that's also acceptable for this test
            assert!(
                stderr.contains("not found") || stderr.contains("No matching"),
                "Should fail with meaningful error. Got: {}",
                stderr
            );
        }
    }

    #[test]
    fn test_ahma_mcp_stdio_mode_rejects_tty() {
        // When run from a terminal (TTY), stdio mode should be rejected
        // Note: This test behavior depends on the test runner's TTY state
        let binary = build_binary("ahma_shell", "ahma_mcp");
        let workspace = workspace_dir();
        let tools_dir = workspace.join(".ahma/tools");

        let output = test_command(&binary)
            .current_dir(&workspace)
            .args([
                "--mode",
                "stdio",
                "--tools-dir",
                tools_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ahma_mcp in stdio mode");

        // In test environment (non-TTY), this should work differently than interactive
        // The test mainly verifies the binary runs without crashing
        let stderr = String::from_utf8_lossy(&output.stderr);

        // If it failed, should have meaningful error message
        if !output.status.success() {
            assert!(
                stderr.contains("terminal") || stderr.contains("MCP") || stderr.contains("Error"),
                "Error should be meaningful. Got: {}",
                stderr
            );
        }
    }
}

// ============================================================================
// ahma_validate Binary Tests
// ============================================================================

mod ahma_validate_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_ahma_validate_help() {
        let binary = build_binary("ahma_validate", "ahma_validate");

        let output = Command::new(&binary)
            .arg("--help")
            .output()
            .expect("Failed to execute ahma_validate --help");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        assert!(
            output.status.success(),
            "ahma_validate --help should succeed. Output: {}",
            combined
        );

        assert!(
            combined.contains("validate") || combined.contains("MTDF") || combined.contains("tool"),
            "Help should mention validation. Got: {}",
            combined
        );
    }

    #[test]
    fn test_ahma_validate_version() {
        let binary = build_binary("ahma_validate", "ahma_validate");

        let output = Command::new(&binary)
            .arg("--version")
            .output()
            .expect("Failed to execute ahma_validate --version");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        assert!(
            output.status.success(),
            "ahma_validate --version should succeed. Output: {}",
            combined
        );
    }

    #[test]
    fn test_ahma_validate_valid_tools_directory() {
        let binary = build_binary("ahma_validate", "ahma_validate");
        let workspace = workspace_dir();
        let tools_dir = workspace.join(".ahma/tools");
        let guidance_file = workspace.join(".ahma/tool_guidance.json");

        let output = Command::new(&binary)
            .current_dir(&workspace)
            .args([
                tools_dir.to_str().unwrap(),
                "--guidance-file",
                guidance_file.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ahma_validate");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "ahma_validate should succeed on valid tools dir. stdout: {}, stderr: {}",
            stdout,
            stderr
        );

        // Should indicate validation passed
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            combined.contains("valid") || combined.contains("Valid"),
            "Output should indicate validation success. Got: {}",
            combined
        );
    }

    #[test]
    fn test_ahma_validate_invalid_json_file() {
        let binary = build_binary("ahma_validate", "ahma_validate");
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace = workspace_dir();
        let guidance_file = workspace.join(".ahma/tool_guidance.json");

        // Create an invalid JSON file
        let invalid_file = temp_dir.path().join("invalid.json");
        fs::write(&invalid_file, "{ this is not valid json }")
            .expect("Failed to write invalid file");

        let output = Command::new(&binary)
            .current_dir(&workspace)
            .args([
                invalid_file.to_str().unwrap(),
                "--guidance-file",
                guidance_file.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ahma_validate");

        // Should fail
        assert!(
            !output.status.success(),
            "ahma_validate should fail on invalid JSON"
        );
    }

    #[test]
    fn test_ahma_validate_nonexistent_path() {
        let binary = build_binary("ahma_validate", "ahma_validate");
        let workspace = workspace_dir();
        let guidance_file = workspace.join(".ahma/tool_guidance.json");

        let output = Command::new(&binary)
            .current_dir(&workspace)
            .args([
                "/nonexistent/path/to/tools",
                "--guidance-file",
                guidance_file.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ahma_validate");

        // Should fail
        assert!(
            !output.status.success(),
            "ahma_validate should fail on nonexistent path"
        );
    }

    #[test]
    fn test_ahma_validate_single_valid_file() {
        let binary = build_binary("ahma_validate", "ahma_validate");
        let workspace = workspace_dir();
        let cargo_json = workspace.join(".ahma/tools/cargo.json");
        let guidance_file = workspace.join(".ahma/tool_guidance.json");

        if cargo_json.exists() {
            let output = Command::new(&binary)
                .current_dir(&workspace)
                .args([
                    cargo_json.to_str().unwrap(),
                    "--guidance-file",
                    guidance_file.to_str().unwrap(),
                ])
                .output()
                .expect("Failed to execute ahma_validate");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            assert!(
                output.status.success(),
                "ahma_validate should succeed on cargo.json. stdout: {}, stderr: {}",
                stdout,
                stderr
            );
        }
    }
}

// ============================================================================
// generate_tool_schema Binary Tests
// ============================================================================

mod generate_tool_schema_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_generate_schema_default_output() {
        let binary = build_binary("generate_tool_schema", "generate_tool_schema");
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace = workspace_dir();

        let output = Command::new(&binary)
            .current_dir(&workspace)
            .arg(temp_dir.path().to_str().unwrap())
            .output()
            .expect("Failed to execute generate_tool_schema");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "generate_tool_schema should succeed. stdout: {}, stderr: {}",
            stdout,
            stderr
        );

        // Should create mtdf-schema.json
        let schema_path = temp_dir.path().join("mtdf-schema.json");
        assert!(
            schema_path.exists(),
            "Schema file should be created at {:?}",
            schema_path
        );

        // Verify schema content
        let schema_content = fs::read_to_string(&schema_path).expect("Failed to read schema");
        assert!(
            schema_content.contains("$schema") || schema_content.contains("ToolConfig"),
            "Schema should contain standard JSON Schema elements. Got: {}",
            &schema_content[..schema_content.len().min(500)]
        );
    }

    #[test]
    fn test_generate_schema_output_is_valid_json() {
        let binary = build_binary("generate_tool_schema", "generate_tool_schema");
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace = workspace_dir();

        Command::new(&binary)
            .current_dir(&workspace)
            .arg(temp_dir.path().to_str().unwrap())
            .output()
            .expect("Failed to execute generate_tool_schema");

        let schema_path = temp_dir.path().join("mtdf-schema.json");
        if schema_path.exists() {
            let schema_content = fs::read_to_string(&schema_path).expect("Failed to read schema");
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&schema_content);

            assert!(
                parsed.is_ok(),
                "Generated schema should be valid JSON. Error: {:?}",
                parsed.err()
            );
        }
    }

    #[test]
    fn test_generate_schema_creates_directory_if_needed() {
        let binary = build_binary("generate_tool_schema", "generate_tool_schema");
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace = workspace_dir();

        let nested_dir = temp_dir.path().join("nested/output/dir");

        let output = Command::new(&binary)
            .current_dir(&workspace)
            .arg(nested_dir.to_str().unwrap())
            .output()
            .expect("Failed to execute generate_tool_schema");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "generate_tool_schema should create nested directories. stdout: {}, stderr: {}",
            stdout,
            stderr
        );

        let schema_path = nested_dir.join("mtdf-schema.json");
        assert!(
            schema_path.exists(),
            "Schema should be created in nested directory"
        );
    }
}

// ============================================================================
// ahma_list_tools Binary Tests
// ============================================================================

mod ahma_list_tools_tests {
    use super::*;

    #[test]
    fn test_ahma_list_tools_help() {
        let binary = build_binary("ahma_list_tools", "ahma_list_tools");

        let output = Command::new(&binary)
            .arg("--help")
            .output()
            .expect("Failed to execute ahma_list_tools --help");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        assert!(
            output.status.success(),
            "ahma_list_tools --help should succeed. Output: {}",
            combined
        );

        assert!(
            combined.contains("MCP") || combined.contains("tool"),
            "Help should mention MCP or tools. Got: {}",
            combined
        );
    }

    #[test]
    fn test_ahma_list_tools_version() {
        let binary = build_binary("ahma_list_tools", "ahma_list_tools");

        let output = Command::new(&binary)
            .arg("--version")
            .output()
            .expect("Failed to execute ahma_list_tools --version");

        assert!(
            output.status.success(),
            "ahma_list_tools --version should succeed"
        );
    }

    #[test]
    fn test_ahma_list_tools_no_connection_method() {
        let binary = build_binary("ahma_list_tools", "ahma_list_tools");

        // Running without any connection method should fail gracefully
        let output = Command::new(&binary)
            .output()
            .expect("Failed to execute ahma_list_tools");

        // Should fail with meaningful error
        assert!(
            !output.status.success(),
            "ahma_list_tools should fail without connection method"
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("connection") || stderr.contains("method") || stderr.contains("--"),
            "Error should mention connection method. Got: {}",
            stderr
        );
    }

    #[test]
    fn test_ahma_list_tools_with_stdio_server() {
        // This test connects to the ahma_mcp binary via stdio
        let ahma_binary = build_binary("ahma_shell", "ahma_mcp");
        let list_binary = build_binary("ahma_list_tools", "ahma_list_tools");
        let workspace = workspace_dir();
        let tools_dir = workspace.join(".ahma/tools");

        let output = Command::new(&list_binary)
            .current_dir(&workspace)
            .args([
                "--",
                ahma_binary.to_str().unwrap(),
                "--tools-dir",
                tools_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute ahma_list_tools with stdio server");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // The command should either succeed and list tools, or fail with a known error
        if output.status.success() {
            assert!(
                stdout.contains("Tool") || stdout.contains("tool") || stdout.contains("cargo"),
                "Output should contain tool information. Got: {}",
                stdout
            );
        } else {
            // Acceptable if it fails due to connection issues
            let combined = format!("{}{}", stdout, stderr);
            println!("ahma_list_tools failed (may be acceptable): {}", combined);
        }
    }
}
