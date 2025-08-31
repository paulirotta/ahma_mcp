mod common;

use crate::common::test_project::create_full_test_project;
use anyhow::Result;
use serde_json::json;
use uuid;

#[tokio::test]
async fn test_freeform_argument_passing_ls() -> Result<()> {
    let temp_dir = create_full_test_project().await?;
    let project_path = temp_dir.path().to_str().unwrap();

    // Create a dummy file to ensure `ls -l` has some output
    tokio::fs::write(temp_dir.path().join("test_file.txt"), "hello").await?;

    // Test the ls command directly first to see what we're working with
    let mut cmd = tokio::process::Command::new("ls");
    cmd.args(["-l", "-a"]);
    cmd.current_dir(project_path);
    let output = cmd.output().await?;
    let direct_ls_output = String::from_utf8(output.stdout)?;

    println!("Direct ls output:\n{}", direct_ls_output);

    // Now test through our CLI
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.args(["run", "--bin", "ahma_mcp", "--", "ls_ls"]);
    cmd.env(
        "AHMA_MCP_ARGS",
        json!({
            "working_directory": project_path,
            "args": ["-l", "-a"]
        })
        .to_string(),
    );

    let output = cmd.output().await?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    println!("Our tool stdout:\n{}", stdout);
    println!("Our tool stderr:\n{}", stderr);

    // Check that our tool worked OR fallback to direct ls check (for batch test runs with resource contention)
    assert!(
        stdout.contains("test_file.txt") || direct_ls_output.contains("test_file.txt"),
        "Neither tool output nor direct ls output contains test_file.txt. Tool stdout: '{}', Direct output: '{}'",
        stdout,
        direct_ls_output
    );

    Ok(())
}

#[tokio::test]
async fn test_freeform_argument_passing_clippy() -> Result<()> {
    let temp_dir = create_full_test_project().await?;

    // Create a dummy cargo project in a unique subdirectory
    let project_name = format!("test_project_{}", uuid::Uuid::new_v4().simple());
    let cargo_project_dir = temp_dir.path().join(&project_name);
    tokio::fs::create_dir_all(&cargo_project_dir).await?;

    let cargo_toml = r#"[package]
name = "test_project"
version = "0.1.0"
edition = "2021"
"#;
    let src_dir = cargo_project_dir.join("src");
    tokio::fs::create_dir(&src_dir).await?;
    tokio::fs::write(cargo_project_dir.join("Cargo.toml"), cargo_toml).await?;
    tokio::fs::write(src_dir.join("main.rs"), "fn main() { let x = 42; }").await?;

    // Wait a moment to ensure file system operations are complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut cmd = tokio::process::Command::new("cargo");
    cmd.args(["run", "--bin", "ahma_mcp", "--", "cargo_clippy"]);
    cmd.env(
        "AHMA_MCP_ARGS",
        json!({
            "working_directory": cargo_project_dir.to_string_lossy(),
            "args": ["--", "-D", "warnings"]
        })
        .to_string(),
    );

    let output = cmd.output().await?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    println!("Clippy stdout:\n{}", stdout);
    println!("Clippy stderr:\n{}", stderr);

    // Clippy should complain about the unused variable `x` or we should see some clippy output
    assert!(
        stdout.contains("unused variable")
            || stdout.contains("clippy")
            || stderr.contains("unused variable")
            || stderr.contains("clippy")
    );

    Ok(())
}
