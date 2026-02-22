use ahma_mcp::config::load_tool_configs;
use ahma_mcp::shell::cli::Cli;
use clap::Parser;
use tempfile::tempdir;

#[tokio::test]
async fn test_load_builtin_tools_async() {
    let temp_dir = tempdir().unwrap();

    let args_rust = vec!["ahma_mcp", "--rust"];
    let cli_rust = Cli::try_parse_from(args_rust).unwrap();
    let configs_rust = load_tool_configs(&cli_rust, Some(temp_dir.path())).await.unwrap();
    assert!(
        configs_rust.contains_key("cargo"),
        "Should load bundled rust.json (named cargo)"
    );

    let args_python = vec!["ahma_mcp", "--python"];
    let cli_python = Cli::try_parse_from(args_python).unwrap();
    let configs_python = load_tool_configs(&cli_python, Some(temp_dir.path()))
        .await
        .unwrap();
    assert!(
        configs_python.contains_key("python"),
        "Should load bundled python.json"
    );

    let args_multiple = vec!["ahma_mcp", "--rust", "--python"];
    let cli_multiple = Cli::try_parse_from(args_multiple).unwrap();
    let configs_multiple = load_tool_configs(&cli_multiple, Some(temp_dir.path()))
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
    let configs = load_tool_configs(&cli, Some(temp_dir.path())).await.unwrap();

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
        let result = load_tool_configs(&cli, Some(temp_dir.path())).await;
        assert!(
            result.is_err(),
            "Reserved name '{}' should be rejected",
            reserved
        );

        // Clean up for next iteration
        std::fs::remove_file(temp_dir.path().join(format!("{}.json", reserved))).unwrap();
    }
}

/// Verify that bundled tools load even when NO tools directory exists.
/// This is the exact scenario when a user runs `ahma_mcp --rust --simplify`
/// from a repo that has no `.ahma/` directory and no `--tools-dir` flag.
#[tokio::test]
async fn test_bundled_tools_load_without_tools_dir() {
    // Pass --rust --simplify but NO --tools-dir, and tools_dir = None
    let cli = Cli::try_parse_from(["ahma_mcp", "--rust", "--simplify"]).unwrap();

    // Call with None — this is the code path that was previously broken
    let configs = load_tool_configs(&cli, None).await.unwrap();

    assert!(
        configs.contains_key("cargo"),
        "--rust flag should load bundled cargo tool even without tools_dir. Got keys: {:?}",
        configs.keys().collect::<Vec<_>>()
    );
    assert!(
        configs.contains_key("simplify"),
        "--simplify flag should load bundled simplify tool even without tools_dir. Got keys: {:?}",
        configs.keys().collect::<Vec<_>>()
    );

    // sandboxed_shell synthetic config should also be present
    assert!(
        configs.contains_key("sandboxed_shell"),
        "sandboxed_shell synthetic config should always be present"
    );
}

/// Verify that each individual bundled flag works without a tools directory.
#[tokio::test]
async fn test_each_bundled_flag_works_without_tools_dir() {
    let flag_and_expected: &[(&str, &str)] = &[
        ("--rust", "cargo"),
        ("--simplify", "simplify"),
        ("--python", "python"),
        ("--git", "git"),
        ("--github", "gh"),
        ("--file", "file_tools"),
    ];

    for &(flag, expected_tool) in flag_and_expected {
        let cli = Cli::try_parse_from(["ahma_mcp", flag]).unwrap();
        let configs = load_tool_configs(&cli, None).await.unwrap();
        assert!(
            configs.contains_key(expected_tool),
            "Flag '{}' should load bundled tool '{}' even without tools_dir. Got keys: {:?}",
            flag,
            expected_tool,
            configs.keys().collect::<Vec<_>>()
        );
    }
}
