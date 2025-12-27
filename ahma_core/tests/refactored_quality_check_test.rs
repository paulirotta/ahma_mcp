use ahma_core::skip_if_disabled;
use ahma_core::test_utils as common;

use common::get_workspace_dir;
use std::process::Command;

#[test]
fn test_run_new_quality_check_sequence() {
    skip_if_disabled!("ahma_quality_check");
    
    // This test runs the ahma_quality_check sequence which includes
    // schema generation and validation specific to the ahma_mcp project.
    let output = Command::new("cargo")
        .current_dir(get_workspace_dir())
        // AHMA_TEST_MODE bypasses sandbox checks in tests
        .env("AHMA_TEST_MODE", "1")
        .env(
            "AHMA_SKIP_SEQUENCE_SUBCOMMANDS",
            "fmt,clippy,nextest_run,build",
        )
        .args([
            "run",
            "--package",
            "ahma_core",
            "--bin",
            "ahma_mcp",
            "--",
            "ahma_quality_check",
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
