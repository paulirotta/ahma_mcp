use anyhow::Result;
use std::path::Path;
use tempfile::TempDir;
use tokio::fs;
use tokio::task::spawn_blocking;

/// Options to customize a temporary project with various tool configurations.
#[derive(Debug, Clone, Default)]
pub struct TestProjectOptions {
    /// Prefix for the temp dir name. A process ID will be appended automatically for uniqueness.
    pub prefix: Option<String>,
    /// Whether to create a Cargo project structure
    pub with_cargo: bool,
    /// Whether to create test files for sed operations
    pub with_text_files: bool,
    /// Whether to include tool configuration files
    pub with_tool_configs: bool,
}

/// Create a temporary project with flexible tool configurations for testing ahma_mcp.
/// Ensures unique directory via tempfile and process ID and never writes to the repo root.
pub async fn create_rust_project(opts: TestProjectOptions) -> Result<TempDir> {
    let process_id = std::process::id();
    let prefix = opts.prefix.unwrap_or_else(|| "ahma_mcp_test_".to_string());

    // TempDir creation is synchronous; use spawn_blocking to keep async threads unblocked under load.
    let temp_dir = spawn_blocking(move || {
        tempfile::Builder::new()
            .prefix(&format!("{}{}_", prefix, process_id))
            .tempdir()
    })
    .await?
    .map_err(anyhow::Error::from)?;

    let project_path = temp_dir.path();

    // Create directory structure based on options
    if opts.with_cargo {
        create_cargo_structure(project_path).await?;
    }

    if opts.with_text_files {
        create_text_files(project_path).await?;
    }

    if opts.with_tool_configs {
        create_tool_configs(project_path).await?;
    }

    Ok(temp_dir)
}

async fn create_cargo_structure(project_path: &Path) -> Result<()> {
    fs::create_dir_all(project_path.join("src")).await?;
    fs::write(
        project_path.join("Cargo.toml"),
        r#"
[package]
name = "project"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
"#,
    )
    .await?;
    fs::write(
        project_path.join("src/main.rs"),
        r#"
#[tokio::main]
async fn main() {
    println!("Hello, world!");
}
"#,
    )
    .await?;
    Ok(())
}

async fn create_text_files(project_path: &Path) -> Result<()> {
    fs::write(project_path.join("test1.txt"), "line1\nline2\nline3\n").await?;
    fs::write(project_path.join("test2.txt"), "foo\nbar\nbaz\n").await?;
    Ok(())
}

async fn create_tool_configs(project_path: &Path) -> Result<()> {
    let tools_dir = project_path.join(".ahma");
    fs::create_dir_all(&tools_dir).await?;
    fs::write(
        tools_dir.join("echo.json"),
        r#"
{
    "name": "echo",
    "description": "Echo a message",
    "command": "echo",
    "timeout_seconds": 10,
    "synchronous": true,
    "enabled": true,
    "subcommand": [
        {
            "name": "default",
            "description": "echo the message",
            "positional_args": [
                {
                    "name": "message",
                    "option_type": "string",
                    "description": "message to echo",
                    "required": true
                }
            ]
        }
    ]
}
"#,
    )
    .await?;
    Ok(())
}

/// Create a temporary project with full Rust project setup for testing
pub async fn create_full_rust_project() -> Result<TempDir> {
    create_rust_project(TestProjectOptions {
        prefix: None,
        with_cargo: true,
        with_text_files: true,
        with_tool_configs: true,
    })
    .await
}
