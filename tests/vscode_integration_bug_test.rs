//! Test to reproduce VS Code MCP integration tool loading issue
//!
//! NOTE: These tests intentionally run actual cargo build commands to test
//! the exact scenario VS Code uses. However, this CAUSES SERVER RESTARTS
//! during AI development sessions when running during nextest.
//!
//! ⚠️  DISABLED: These tests cause the ahma_mcp server to restart during test runs,
//! interrupting AI development. They should be run manually when needed to test
//! VS Code integration, not as part of the automated test suite.

use anyhow::Result;
use std::process::Command;

#[tokio::test]
#[ignore] // Disabled: causes server restarts during test runs
async fn test_vscode_tool_loading_scenario() -> Result<()> {
    // First, ensure we have a release build (VS Code uses this)
    let build_output = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(".")
        .output()?;

    assert!(
        build_output.status.success(),
        "Release build failed: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Test the exact command VS Code runs with the new .ahma/tools path
    let server_output = Command::new("./target/release/ahma_mcp")
        .args(["--server", "--tools-dir", ".ahma/tools"])
        .current_dir(".") // Same as VS Code's workspaceFolder
        .env("RUST_LOG", "info")
        .output()?;

    let stderr_content = String::from_utf8_lossy(&server_output.stderr);
    println!("Server stderr: {}", stderr_content);

    // This should NOT contain the error message VS Code is seeing
    assert!(
        !stderr_content.contains("No valid tool configurations found"),
        "Server failed to find tool configurations:\n{}",
        stderr_content
    );

    // This should contain the success message
    assert!(
        stderr_content.contains("Loaded") && stderr_content.contains("tool configurations"),
        "Server did not report loading tool configurations:\n{}",
        stderr_content
    );

    Ok(())
}

#[tokio::test]
#[ignore] // Disabled: causes server restarts during test runs
async fn test_vscode_dev_tool_loading_scenario() -> Result<()> {
    // Test the exact dev command VS Code might run with correct path
    let server_output = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--bin",
            "ahma_mcp",
            "--",
            "--server",
            "--tools-dir",
            ".ahma/tools",
        ])
        .current_dir(".") // Same as VS Code's workspaceFolder
        .env("RUST_LOG", "info")
        .output()?;

    let stderr_content = String::from_utf8_lossy(&server_output.stderr);
    println!("Dev server stderr: {}", stderr_content);

    // This should NOT contain the error message VS Code is seeing
    assert!(
        !stderr_content.contains("No valid tool configurations found"),
        "Dev server failed to find tool configurations:\n{}",
        stderr_content
    );

    // This should contain the success message
    assert!(
        stderr_content.contains("Loaded") && stderr_content.contains("tool configurations"),
        "Dev server did not report loading tool configurations:\n{}",
        stderr_content
    );

    Ok(())
}
