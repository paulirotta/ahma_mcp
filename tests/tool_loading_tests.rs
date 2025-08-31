//! Integration tests for tool loading and configuration
//!
//! These tests verify that ahma_mcp can correctly load and parse tool configurations
//! from TOML files and initialize the adapter properly.

use ahma_mcp::{Adapter, Config};
use anyhow::Result;
use common::test_project::{TestProjectOptions, create_test_project};
use common::test_utils::{dir_exists, file_exists};

mod common;

/// Test that the adapter can be created with basic configuration
#[tokio::test]
async fn test_adapter_creation() -> Result<()> {
    let adapter = Adapter::new(true)?; // synchronous mode
    assert!(adapter.get_tool_schemas()?.is_empty()); // No tools loaded yet
    Ok(())
}

/// Test that the adapter can be created with timeout
#[tokio::test]
async fn test_adapter_creation_with_timeout() -> Result<()> {
    let adapter = Adapter::with_timeout(false, 120)?; // async mode, 120 second timeout
    assert!(adapter.get_tool_schemas()?.is_empty()); // No tools loaded yet
    Ok(())
}

/// Test loading tools from a temporary tools directory
#[tokio::test]
async fn test_load_tools_from_directory() -> Result<()> {
    let temp_project = create_test_project(TestProjectOptions::default()).await?;
    let project_path = temp_project.path();
    let tools_dir = project_path.join("tools");
    tokio::fs::create_dir_all(&tools_dir).await?;

    // Create tool configs using commands that should be available on most systems
    let curl_config = r#"tool_name = "curl"
command = "curl"
enabled = true
timeout_seconds = 30

[hints]
primary = "HTTP client for making requests"
usage = "curl https://example.com"
"#;

    let git_config = r#"tool_name = "git"
command = "git"
enabled = true
timeout_seconds = 30

[hints]
primary = "Git version control system"
usage = "git status, git add, git commit"
"#;

    tokio::fs::write(tools_dir.join("curl.toml"), curl_config).await?;
    tokio::fs::write(tools_dir.join("git.toml"), git_config).await?;

    // Verify the tools directory was created
    assert!(dir_exists(&tools_dir).await);
    assert!(file_exists(&tools_dir.join("curl.toml")).await);
    assert!(file_exists(&tools_dir.join("git.toml")).await);

    // Create adapter and initialize with the tools directory
    let mut adapter = Adapter::new(true)?;

    // Manually add tools using absolute paths instead of changing directory
    for entry in std::fs::read_dir(&tools_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let tool_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let config = Config::load_from_file(&path)?;
            if config.is_enabled() {
                adapter.add_tool(&tool_name, config).await?;
            }
        }
    }

    // Verify tools were loaded
    let schemas = adapter.get_tool_schemas()?;
    assert!(!schemas.is_empty(), "Should have loaded tool schemas");

    Ok(())
}

/// Test loading a single tool configuration
#[tokio::test]
async fn test_load_single_tool_config() -> Result<()> {
    let temp_project = create_test_project(TestProjectOptions {
        with_tool_configs: true,
        ..Default::default()
    })
    .await?;

    let project_path = temp_project.path();
    let curl_config_path = project_path.join("tools").join("curl.toml");

    // Load the curl tool configuration
    let config = Config::load_from_file(&curl_config_path)?;

    // Verify the configuration was loaded correctly
    assert_eq!(config.tool_name, "curl");
    assert_eq!(config.command, Some("curl".to_string()));
    assert_eq!(config.enabled, Some(true));
    assert_eq!(config.timeout_seconds, Some(30));

    // Verify hints were loaded
    assert!(config.hints.is_some());
    let hints = config.hints.unwrap();
    assert!(hints.primary.is_some());
    assert!(hints.primary.unwrap().contains("HTTP client"));

    Ok(())
}

/// Test that invalid tool configurations are handled gracefully
#[tokio::test]
async fn test_invalid_tool_config_handling() -> Result<()> {
    let temp_project = create_test_project(TestProjectOptions::default()).await?;
    let project_path = temp_project.path();
    let tools_dir = project_path.join("tools");
    tokio::fs::create_dir_all(&tools_dir).await?;

    // Create an invalid TOML file
    let invalid_toml = r#"
[invalid_section
missing_quote = "test
"#;
    tokio::fs::write(tools_dir.join("invalid.toml"), invalid_toml).await?;

    // Create adapter and try to initialize
    let mut adapter = Adapter::new(true)?;

    // Change to project directory
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(project_path)?;

    // Initialize should handle invalid configs gracefully (not crash)
    let result = adapter.initialize().await;

    // Restore directory
    std::env::set_current_dir(original_dir)?;

    // Should either succeed (ignoring invalid file) or fail gracefully
    match result {
        Ok(_) => {
            // If it succeeds, make sure no tools were loaded from the invalid file
            let schemas = adapter.get_tool_schemas()?;
            // Should be empty since only invalid config exists
            assert!(schemas.is_empty());
        }
        Err(e) => {
            // If it fails, should be a parse error, not a panic
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("parse")
                    || error_msg.contains("Invalid")
                    || error_msg.contains("TOML")
                    || error_msg.contains("expected")
                    || error_msg.contains("EOF"),
                "Expected parsing error, got: {}",
                error_msg
            );
        }
    }
    Ok(())
}

/// Test tool configuration with overrides
#[tokio::test]
async fn test_tool_config_with_overrides() -> Result<()> {
    let temp_project = create_test_project(TestProjectOptions::default()).await?;
    let project_path = temp_project.path();
    let tools_dir = project_path.join("tools");
    tokio::fs::create_dir_all(&tools_dir).await?;

    // Create a config with overrides
    let config_with_overrides = r#"
tool_name = "curl"
command = "curl"
enabled = true
timeout_seconds = 30

[hints]
primary = "HTTP operations"

[overrides.get]
timeout_seconds = 15
hints.primary = "HTTP GET request"

[overrides.post]
timeout_seconds = 60
hints.primary = "HTTP POST request"
"#;

    tokio::fs::write(tools_dir.join("curl.toml"), config_with_overrides).await?;

    // Load and verify the configuration
    let config = Config::load_from_file(tools_dir.join("curl.toml"))?;

    // Verify base config
    assert_eq!(config.timeout_seconds, Some(30));

    // Verify overrides were loaded
    assert!(config.overrides.is_some());
    let overrides = config.overrides.unwrap();

    assert!(overrides.contains_key("get"));
    assert!(overrides.contains_key("post"));

    let get_override = &overrides["get"];
    assert_eq!(get_override.timeout_seconds, Some(15));

    let post_override = &overrides["post"];
    assert_eq!(post_override.timeout_seconds, Some(60));

    Ok(())
}

/// Test that disabled tools are not loaded
#[tokio::test]
async fn test_disabled_tools_not_loaded() -> Result<()> {
    let temp_project = create_test_project(TestProjectOptions::default()).await?;
    let project_path = temp_project.path();
    let tools_dir = project_path.join("tools");
    tokio::fs::create_dir_all(&tools_dir).await?;

    // Create enabled and disabled tool configs using real commands
    let enabled_config = r#"tool_name = "curl"
command = "curl"
enabled = true
"#;

    let disabled_config = r#"tool_name = "disabled_curl" 
command = "curl"
enabled = false
"#;

    tokio::fs::write(tools_dir.join("curl.toml"), enabled_config).await?;
    tokio::fs::write(tools_dir.join("disabled.toml"), disabled_config).await?;

    // Initialize adapter manually instead of changing directory
    let mut adapter = Adapter::new(true)?;

    // Manually load tools to avoid directory changing issues
    for entry in std::fs::read_dir(&tools_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let tool_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let config = Config::load_from_file(&path)?;
            if config.is_enabled() {
                adapter.add_tool(&tool_name, config).await?;
            }
        }
    }

    // Verify only enabled tools are in schemas
    let schemas = adapter.get_tool_schemas()?;
    assert_eq!(schemas.len(), 1, "Should only have one enabled tool");

    // The schema should be for the curl tool
    let schema_str = serde_json::to_string(&schemas[0])?;
    assert!(schema_str.contains("curl"));
    assert!(!schema_str.contains("disabled_curl"));

    Ok(())
}
