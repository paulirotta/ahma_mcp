use ahma_mcp::config::{load_tool_configs, load_tool_configs_sync};
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

#[test]
fn test_load_builtin_tools_sync() {
    let temp_dir = tempdir().unwrap();

    let args_shell = vec!["ahma_mcp", "--shell"];
    let cli_shell = Cli::try_parse_from(args_shell).unwrap();
    let configs_shell = load_tool_configs_sync(&cli_shell, temp_dir.path()).unwrap();
    assert!(
        configs_shell.contains_key("sandboxed_shell"),
        "Should load bundled sandboxed_shell.json"
    );
}
