//! Nested Sandbox Exit Test (R7.6)
//!
//! This test verifies that when ahma_mcp is running inside another sandbox,
//! it exits with a clear error message instructing the user to use --no-sandbox.
//!
//! Per R7.6.2: "Upon detection, the system **must** exit with a clear error message
//! instructing the user to disable the internal sandbox using the --no-sandbox flag
//! or AHMA_NO_SANDBOX=1 environment variable."
//!
//! ## Running These Tests
//!
//! These tests require the ability to run `sandbox-exec`. If the test runner itself
//! is inside a sandbox (e.g., Cursor IDE, certain CI environments), the tests will
//! be skipped automatically.
//!
//! To run manually:
//! ```bash
//! # From a non-sandboxed terminal (e.g., iTerm, Terminal.app)
//! cargo test --package ahma_core --test nested_sandbox_exit_test
//! ```

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::Command;

/// Check if sandbox-exec can be run at all.
/// Returns false if we're inside another sandbox that prevents sandbox-exec.
fn can_run_sandbox_exec() -> bool {
    let result = Command::new("sandbox-exec")
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .output();

    match result {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Get the workspace directory (project root)
fn get_workspace_dir() -> PathBuf {
    std::env::current_dir()
        .expect("Failed to get current directory")
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists() && p.join("ahma_core").exists())
        .expect("Could not find workspace root")
        .to_path_buf()
}

/// Build the ahma_mcp binary and return its path
fn build_ahma_mcp_binary() -> PathBuf {
    let workspace_dir = get_workspace_dir();

    let output = Command::new("cargo")
        .current_dir(&workspace_dir)
        .args(["build", "--package", "ahma_core", "--bin", "ahma_mcp"])
        .output()
        .expect("Failed to build ahma_mcp");

    if !output.status.success() {
        panic!(
            "Failed to build ahma_mcp:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    workspace_dir
        .join("target")
        .join("debug")
        .join("ahma_mcp")
}

/// Skip helper macro - prints message and returns early if sandbox-exec unavailable
macro_rules! skip_if_sandboxed {
    () => {
        if !can_run_sandbox_exec() {
            eprintln!(
                "SKIPPED: Test runner is inside a sandbox - cannot test nested sandbox behavior."
            );
            eprintln!("Run this test from a non-sandboxed terminal (e.g., iTerm, Terminal.app)");
            return;
        }
    };
}

/// Test that ahma_mcp exits with error when launched inside a sandbox (R7.6)
///
/// This test wraps ahma_mcp in sandbox-exec, which triggers the nested sandbox detection.
/// The expected behavior is:
/// 1. Process exits with non-zero code
/// 2. stderr contains "SECURITY ERROR" or "nested sandbox"
/// 3. stderr provides instructions about --no-sandbox or AHMA_NO_SANDBOX
#[test]
fn test_nested_sandbox_detection_exits_with_error() {
    skip_if_sandboxed!();
    let binary = build_ahma_mcp_binary();
    let workspace_dir = get_workspace_dir();

    // Create a permissive sandbox profile that allows ahma_mcp to run
    // but will cause *its* nested sandbox detection to fail
    let outer_sandbox_profile = "(version 1)(allow default)";

    // Run ahma_mcp inside sandbox-exec - this triggers nested sandbox detection
    let output = Command::new("sandbox-exec")
        .current_dir(&workspace_dir)
        .args([
            "-p",
            outer_sandbox_profile,
            binary.to_str().unwrap(),
            // Use list-tools mode which is quick and will trigger the sandbox check
            "--mode",
            "list-tools",
            "--tools-dir",
            ".ahma/tools",
        ])
        .output()
        .expect("Failed to spawn ahma_mcp inside sandbox");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug output for test failures
    eprintln!("Exit code: {:?}", output.status.code());
    eprintln!("stderr:\n{}", stderr);
    eprintln!("stdout:\n{}", stdout);

    // Assert: process exits with non-zero code (per R7.6.2)
    assert!(
        !output.status.success(),
        "ahma_mcp should exit with non-zero when nested sandbox is detected"
    );

    // Assert: stderr contains security error message
    assert!(
        stderr.contains("SECURITY ERROR") || stderr.contains("nested sandbox"),
        "Error message should mention SECURITY ERROR or nested sandbox. Got:\n{}",
        stderr
    );

    // Assert: stderr provides instructions about --no-sandbox
    assert!(
        stderr.contains("--no-sandbox") || stderr.contains("AHMA_NO_SANDBOX"),
        "Error message should mention --no-sandbox or AHMA_NO_SANDBOX. Got:\n{}",
        stderr
    );
}

/// Test that ahma_mcp works normally with --no-sandbox when inside a sandbox (R7.6)
///
/// When the user explicitly disables the sandbox with --no-sandbox,
/// ahma_mcp should run successfully even inside another sandbox.
#[test]
fn test_no_sandbox_flag_allows_nested_execution() {
    skip_if_sandboxed!();
    let binary = build_ahma_mcp_binary();
    let workspace_dir = get_workspace_dir();

    let outer_sandbox_profile = "(version 1)(allow default)";

    // Run ahma_mcp inside sandbox-exec with --no-sandbox
    let output = Command::new("sandbox-exec")
        .current_dir(&workspace_dir)
        .args([
            "-p",
            outer_sandbox_profile,
            binary.to_str().unwrap(),
            "--no-sandbox", // Explicitly disable sandbox
            "--mode",
            "list-tools",
            "--tools-dir",
            ".ahma/tools",
        ])
        .output()
        .expect("Failed to spawn ahma_mcp inside sandbox with --no-sandbox");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug output for test failures
    eprintln!("Exit code: {:?}", output.status.code());
    eprintln!("stderr:\n{}", stderr);
    eprintln!("stdout:\n{}", stdout);

    // With --no-sandbox, the process should succeed (list-tools outputs JSON)
    assert!(
        output.status.success(),
        "ahma_mcp should succeed with --no-sandbox even inside another sandbox. stderr:\n{}",
        stderr
    );

    // stdout should contain JSON output (tool list)
    assert!(
        stdout.contains('[') || stdout.contains('{'),
        "Expected JSON output from list-tools mode. Got:\n{}",
        stdout
    );
}

/// Test that AHMA_NO_SANDBOX=1 env var allows nested execution (R7.6)
#[test]
fn test_no_sandbox_env_var_allows_nested_execution() {
    skip_if_sandboxed!();
    let binary = build_ahma_mcp_binary();
    let workspace_dir = get_workspace_dir();

    let outer_sandbox_profile = "(version 1)(allow default)";

    // Run ahma_mcp inside sandbox-exec with AHMA_NO_SANDBOX=1
    let output = Command::new("sandbox-exec")
        .current_dir(&workspace_dir)
        .env("AHMA_NO_SANDBOX", "1")
        .args([
            "-p",
            outer_sandbox_profile,
            binary.to_str().unwrap(),
            "--mode",
            "list-tools",
            "--tools-dir",
            ".ahma/tools",
        ])
        .output()
        .expect("Failed to spawn ahma_mcp inside sandbox with AHMA_NO_SANDBOX=1");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug output for test failures
    eprintln!("Exit code: {:?}", output.status.code());
    eprintln!("stderr:\n{}", stderr);
    eprintln!("stdout:\n{}", stdout);

    // With AHMA_NO_SANDBOX=1, the process should succeed
    assert!(
        output.status.success(),
        "ahma_mcp should succeed with AHMA_NO_SANDBOX=1 even inside another sandbox. stderr:\n{}",
        stderr
    );
}
