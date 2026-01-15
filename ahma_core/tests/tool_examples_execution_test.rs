//! Tool Examples Execution Integration Tests
//!
//! This test module runs each tool configuration example to ensure they execute
//! successfully and produce valid output.

use std::process::Command;

/// Helper function to run an example and verify it succeeds
fn run_example(example_name: &str) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["run", "--example", example_name])
        .output()
        .map_err(|e| format!("Failed to execute example {}: {}", example_name, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "Example {} failed with exit code {:?}\nstdout:\n{}\nstderr:\n{}",
            example_name,
            output.status.code(),
            stdout,
            stderr
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}

/// Helper function to verify expected content in output
fn assert_output_contains(output: &str, expected: &str, example_name: &str) {
    assert!(
        output.contains(expected),
        "Example {} output should contain '{}'\nActual output:\n{}",
        example_name,
        expected,
        output
    );
}

#[test]
fn test_cargo_tool_example_runs() {
    let output = run_example("cargo_tool").expect("cargo_tool example should run successfully");

    assert_output_contains(&output, "✅ Configuration is valid!", "cargo_tool");
    assert_output_contains(&output, "Name: cargo", "cargo_tool");
    assert_output_contains(&output, "Command: cargo", "cargo_tool");
    assert_output_contains(&output, "Enabled: true", "cargo_tool");
}

#[test]
fn test_file_tools_example_runs() {
    let output = run_example("file_tools").expect("file_tools example should run successfully");

    assert_output_contains(&output, "✅ Configuration is valid!", "file_tools");
    assert_output_contains(&output, "Name: file_tools", "file_tools");
    assert_output_contains(&output, "Command: /bin/sh", "file_tools");
    assert_output_contains(&output, "Enabled: true", "file_tools");
}

#[test]
fn test_gh_tool_example_runs() {
    let output = run_example("gh_tool").expect("gh_tool example should run successfully");

    assert_output_contains(&output, "✅ Configuration is valid!", "gh_tool");
    assert_output_contains(&output, "Name: gh", "gh_tool");
    assert_output_contains(&output, "Command: gh", "gh_tool");
    assert_output_contains(&output, "Enabled: true", "gh_tool");
}

#[test]
fn test_git_tool_example_runs() {
    let output = run_example("git_tool").expect("git_tool example should run successfully");

    assert_output_contains(&output, "✅ Configuration is valid!", "git_tool");
    assert_output_contains(&output, "Name: git", "git_tool");
    assert_output_contains(&output, "Command: git", "git_tool");
    assert_output_contains(&output, "Enabled: true", "git_tool");
}

#[test]
fn test_gradlew_tool_example_runs() {
    let output = run_example("gradlew_tool").expect("gradlew_tool example should run successfully");

    assert_output_contains(&output, "✅ Configuration is valid!", "gradlew_tool");
    assert_output_contains(&output, "Name: gradlew", "gradlew_tool");
    assert_output_contains(&output, "Command: ./gradlew", "gradlew_tool");
    assert_output_contains(&output, "Enabled: true", "gradlew_tool");
}

#[test]
fn test_python_tool_example_runs() {
    let output = run_example("python_tool").expect("python_tool example should run successfully");

    assert_output_contains(&output, "✅ Configuration is valid!", "python_tool");
    assert_output_contains(&output, "Name: python", "python_tool");
    assert_output_contains(&output, "Command: python", "python_tool");
    assert_output_contains(&output, "Enabled: true", "python_tool");
}

#[test]
fn test_all_examples_show_subcommands() {
    let examples = [
        "cargo_tool",
        "file_tools",
        "gh_tool",
        "git_tool",
        "gradlew_tool",
        "python_tool",
    ];

    for example in &examples {
        let output = run_example(example).unwrap_or_else(|_| panic!("{} should run successfully", example));
        assert_output_contains(&output, "🔧 Available Subcommands:", example);
    }
}
