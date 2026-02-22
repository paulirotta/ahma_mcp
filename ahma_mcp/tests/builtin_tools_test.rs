use ahma_mcp::config::load_tool_configs;
use ahma_mcp::shell::cli::Cli;
use clap::Parser;
use tempfile::tempdir;

#[tokio::test]
async fn test_load_builtin_tools_async() {
    let temp_dir = tempdir().unwrap();

    let args_rust = vec!["ahma_mcp", "--rust"];
    let cli_rust = Cli::try_parse_from(args_rust).unwrap();
    let configs_rust = load_tool_configs(&cli_rust, temp_dir.path()).await.unwrap();
    assert!(
        configs_rust.contains_key("cargo"),
        "Should load bundled rust.json (named cargo)"
    );

    let args_python = vec!["ahma_mcp", "--python"];
    let cli_python = Cli::try_parse_from(args_python).unwrap();
    let configs_python = load_tool_configs(&cli_python, temp_dir.path())
        .await
        .unwrap();
    assert!(
        configs_python.contains_key("python"),
        "Should load bundled python.json"
    );

    let args_multiple = vec!["ahma_mcp", "--rust", "--python"];
    let cli_multiple = Cli::try_parse_from(args_multiple).unwrap();
    let configs_multiple = load_tool_configs(&cli_multiple, temp_dir.path())
        .await
        .unwrap();
    assert!(configs_multiple.contains_key("cargo"), "Should load cargo");
    assert!(
        configs_multiple.contains_key("python"),
        "Should load python"
    );
}

/// Verify that a user-provided .ahma/ tool definition overrides the bundled version.
#[tokio::test]
async fn test_filesystem_overrides_bundled_tool() {
    let temp_dir = tempdir().unwrap();

    // Create a local rust.json that defines "cargo" with a custom description
    let custom_cargo = r#"{
  "name": "cargo",
  "description": "Custom user-defined cargo tool",
  "command": "cargo",
  "enabled": true,
  "subcommand": [
    {
      "name": "build",
      "description": "Custom build",
      "options": [
        {
          "name": "release",
          "type": "boolean",
          "description": "Release mode"
        }
      ]
    }
  ]
}"#;
    std::fs::write(temp_dir.path().join("rust.json"), custom_cargo).unwrap();

    // Load with --rust flag (bundled) AND the local override
    let cli = Cli::try_parse_from(["ahma_mcp", "--rust"]).unwrap();
    let configs = load_tool_configs(&cli, temp_dir.path()).await.unwrap();

    assert!(configs.contains_key("cargo"), "Should have cargo tool");
    let cargo = &configs["cargo"];
    assert_eq!(
        cargo.description, "Custom user-defined cargo tool",
        "Local .ahma/ definition should override the bundled version"
    );
}

/// Verify that reserved tool names (core built-in tools) are rejected from .ahma/ files.
#[tokio::test]
async fn test_reserved_names_rejected() {
    let temp_dir = tempdir().unwrap();

    for reserved in &["await", "status", "sandboxed_shell", "cancel"] {
        let config = format!(
            r#"{{
  "name": "{}",
  "description": "Should be rejected",
  "command": "echo",
  "enabled": true,
  "subcommand": [{{ "name": "default", "description": "test" }}]
}}"#,
            reserved
        );
        std::fs::write(temp_dir.path().join(format!("{}.json", reserved)), &config).unwrap();

        let cli = Cli::try_parse_from(["ahma_mcp"]).unwrap();
        let result = load_tool_configs(&cli, temp_dir.path()).await;
        assert!(
            result.is_err(),
            "Reserved name '{}' should be rejected",
            reserved
        );

        // Clean up for next iteration
        std::fs::remove_file(temp_dir.path().join(format!("{}.json", reserved))).unwrap();
    }
}
