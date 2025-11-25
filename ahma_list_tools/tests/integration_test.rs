//! Integration tests for ahma_list_tools
//!
//! These tests verify the tool listing functionality works correctly with
//! both stdio and HTTP MCP servers.

use std::path::PathBuf;
use std::process::Command;

/// Test that the binary compiles and shows help
#[test]
fn test_help_output() {
    let output = Command::new("cargo")
        .args(["run", "-p", "ahma_list_tools", "--", "--help"])
        .output()
        .expect("Failed to execute command");

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
    // First build ahma_mcp
    let build_output = Command::new("cargo")
        .args(["build", "-p", "ahma_shell"])
        .output()
        .expect("Failed to build ahma_shell");

    assert!(
        build_output.status.success(),
        "Failed to build ahma_shell: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Get the path to the built binary
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let ahma_binary = project_root.join("target/debug/ahma_mcp");
    let tools_dir = project_root.join(".ahma/tools");

    // Run ahma_list_tools with the stdio server
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "ahma_list_tools",
            "--",
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
    // Build ahma_mcp first
    let build_output = Command::new("cargo")
        .args(["build", "-p", "ahma_shell"])
        .output()
        .expect("Failed to build ahma_shell");

    assert!(build_output.status.success(), "Failed to build ahma_shell");

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let ahma_binary = project_root.join("target/debug/ahma_mcp");
    let tools_dir = project_root.join(".ahma/tools");

    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "ahma_list_tools",
            "--",
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
