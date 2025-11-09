mod common;

use common::get_workspace_dir;
use std::process::Command;

#[test]
fn test_run_new_quality_check_sequence() {
    // This test runs the new quality check sequence which should
    // include schema generation and validation.
    let output = Command::new("cargo")
        .current_dir(get_workspace_dir())
        .args([
            "run",
            "--package",
            "ahma_shell",
            "--bin",
            "ahma_mcp",
            "--",
            "rust_quality_check",
        ])
        .output()
        .expect("Failed to execute quality check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Quality check failed.\nStdout: {}\nStderr: {}",
        stdout,
        stderr
    );

    // Check for output from the new steps
    assert!(
        stdout.contains("Generated MTDF JSON Schema")
            || stderr.contains("Generated MTDF JSON Schema"),
        "Schema generation step did not run."
    );
    assert!(
        stdout.contains("All configurations are valid")
            || stderr.contains("All configurations are valid"),
        "Validation step did not report success."
    );
}
