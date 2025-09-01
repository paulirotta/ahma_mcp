#![allow(dead_code)]

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
pub async fn create_test_project(opts: TestProjectOptions) -> Result<TempDir> {
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

/// Create a basic Cargo project structure
async fn create_cargo_structure(project_path: &Path) -> Result<()> {
    let cargo_toml = r#"[package]
name = "test_project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;

    let src_dir = project_path.join("src");
    fs::create_dir_all(&src_dir).await?;

    let main_rs = r#"fn main() {
    println!("Hello, test world!");
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
"#;

    fs::write(project_path.join("Cargo.toml"), cargo_toml).await?;
    fs::write(src_dir.join("main.rs"), main_rs).await?;

    Ok(())
}

/// Create text files for sed operations testing
async fn create_text_files(project_path: &Path) -> Result<()> {
    let sample_text = r#"This is line 1
This is line 2 with some text
Another line with different content
Final line for testing
"#;

    let data_file = r#"name=John
age=30
city=New York
country=USA
"#;

    fs::write(project_path.join("sample.txt"), sample_text).await?;
    fs::write(project_path.join("data.txt"), data_file).await?;

    Ok(())
}

/// Create tool configuration files for testing
async fn create_tool_configs(project_path: &Path) -> Result<()> {
    let tools_dir = project_path.join("tools");
    fs::create_dir_all(&tools_dir).await?;

    // Create a basic curl config (curl should be available on most systems)
    let curl_config = r#"tool_name = "curl"
command = "curl"
enabled = true
timeout_seconds = 30

[hints]
primary = "HTTP client for making requests"
usage = "curl https://example.com"
"#;
    fs::write(tools_dir.join("curl.toml"), curl_config).await?;
    Ok(())
}

/// Convenience wrappers for common project types
pub async fn create_basic_project() -> Result<TempDir> {
    create_test_project(TestProjectOptions::default()).await
}

pub async fn create_cargo_project() -> Result<TempDir> {
    create_test_project(TestProjectOptions {
        with_cargo: true,
        ..Default::default()
    })
    .await
}

pub async fn create_full_test_project() -> Result<TempDir> {
    create_test_project(TestProjectOptions {
        with_cargo: true,
        with_text_files: true,
        with_tool_configs: true,
        ..Default::default()
    })
    .await
}
