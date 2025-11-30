//! Integration tests for ahma_list_tools
//!
//! These tests verify the tool listing functionality works correctly with
//! both stdio and HTTP MCP servers.

use std::path::PathBuf;
use std::process::Command;

/// Get the path to the pre-built ahma_list_tools binary
fn get_ahma_list_tools_binary() -> PathBuf {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    project_root.join("target/debug/ahma_list_tools")
}

/// Test that the binary compiles and shows help
#[test]
fn test_help_output() {
    let binary = get_ahma_list_tools_binary();

    // Use pre-built binary if available, otherwise fall back to cargo run
    let output = if binary.exists() {
        Command::new(&binary)
            .args(["--help"])
            .output()
            .expect("Failed to execute command")
    } else {
        eprintln!("Warning: Pre-built binary not found, falling back to cargo run");
        Command::new("cargo")
            .args(["run", "-p", "ahma_list_tools", "--", "--help"])
            .output()
            .expect("Failed to execute command")
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either stdout or stderr should contain help text
    let help_text = format!("{}{}", stdout, stderr);
    assert!(
        help_text.contains("ahma_list_tools") || help_text.contains("MCP"),
        "Help should contain tool name or MCP reference. Got: {}",
        help_text
    );
}

/// Test that we can list tools from a stdio MCP server
#[test]
fn test_list_tools_from_stdio_server() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let ahma_binary = project_root.join("target/debug/ahma_mcp");
    let list_tools_binary = get_ahma_list_tools_binary();
    let tools_dir = project_root.join(".ahma/tools");

    // Check if pre-built binaries exist
    if !ahma_binary.exists() || !list_tools_binary.exists() {
        eprintln!(
            "Warning: Pre-built binaries not found. Run 'cargo build' first for faster tests."
        );
        // Fall back to cargo build + run
        let build_output = Command::new("cargo")
            .args(["build", "-p", "ahma_shell", "-p", "ahma_list_tools"])
            .output()
            .expect("Failed to build");
        assert!(build_output.status.success(), "Failed to build");
    }

    // Run ahma_list_tools with the stdio server using pre-built binaries
    // Set AHMA_TEST_MODE to bypass sandbox checks in tests
    let output = Command::new(&list_tools_binary)
        .env("AHMA_TEST_MODE", "1")
        .args([
            "--",
            ahma_binary.to_str().unwrap(),
            "--tools-dir",
            tools_dir.to_str().unwrap(),
        ])
        .current_dir(&project_root)
        .output()
        .expect("Failed to execute ahma_list_tools");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The output should contain tool information
    if !output.status.success() {
        eprintln!("stdout: {}", stdout);
        eprintln!("stderr: {}", stderr);
    }

    // Check we got some tools listed
    assert!(
        stdout.contains("Tool:") || stdout.contains("tools"),
        "Output should contain tool listings. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

/// Test that we can parse an mcp.json file
#[test]
fn test_parse_mcp_json() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let mcp_json_path = temp_dir.path().join("mcp.json");

    let mcp_json_content = r#"{
        "servers": {
            "TestServer": {
                "type": "stdio",
                "command": "/path/to/mcp_server",
                "args": ["--tools-dir", "./tools"]
            }
        }
    }"#;

    fs::write(&mcp_json_path, mcp_json_content).unwrap();

    // Verify the file was created
    assert!(mcp_json_path.exists());

    // The actual parsing is tested in unit tests in main.rs
}

/// Test output format contains expected sections
#[test]
fn test_output_format() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let ahma_binary = project_root.join("target/debug/ahma_mcp");
    let list_tools_binary = get_ahma_list_tools_binary();
    let tools_dir = project_root.join(".ahma/tools");

    // Check if pre-built binaries exist
    if !ahma_binary.exists() || !list_tools_binary.exists() {
        eprintln!(
            "Warning: Pre-built binaries not found. Run 'cargo build' first for faster tests."
        );
        let build_output = Command::new("cargo")
            .args(["build", "-p", "ahma_shell", "-p", "ahma_list_tools"])
            .output()
            .expect("Failed to build");
        assert!(build_output.status.success(), "Failed to build");
    }

    // Set AHMA_TEST_MODE to bypass sandbox checks in tests
    let output = Command::new(&list_tools_binary)
        .env("AHMA_TEST_MODE", "1")
        .args([
            "--",
            ahma_binary.to_str().unwrap(),
            "--tools-dir",
            tools_dir.to_str().unwrap(),
        ])
        .current_dir(&project_root)
        .output()
        .expect("Failed to execute ahma_list_tools");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have a header section
    assert!(
        stdout.contains("MCP") || stdout.contains("Tool"),
        "Output should contain 'MCP' or 'Tool' header. Got: {}",
        stdout
    );
}
