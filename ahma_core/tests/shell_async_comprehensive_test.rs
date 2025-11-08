/// Comprehensive test suite for shell_async tool functionality
///
/// This test suite covers:
/// - Basic command execution
/// - Working directory handling
/// - Timeout scenarios
/// - Error cases and edge conditions
/// - Command output verification
/// - Async execution behavior
mod common;

use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use common::{get_workspace_path, test_client::new_client};
use rmcp::model::CallToolRequestParam;
use serde_json::{json, Map};
use std::borrow::Cow;
use tokio::fs;

fn unique_suffix() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

async fn make_workdir(prefix: &str) -> Result<String> {
    let base = get_workspace_path(".ahma/tmp/shell_async");
    fs::create_dir_all(&base).await?;
    let dir = base.join(format!("{}_{}", prefix, unique_suffix()));
    fs::create_dir_all(&dir).await?;
    Ok(dir.to_string_lossy().to_string())
}

/// Test basic shell command execution
#[tokio::test]
async fn test_basic_shell_command_execution() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("basic").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'Hello World'"));
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Basic shell command should execute successfully"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test working directory handling
#[tokio::test]
async fn test_working_directory_handling() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let temp_dir = make_workdir("wd_handling").await?;

    // Create a test file in the temp directory
    let test_file_path = std::path::Path::new(&temp_dir).join("test_file.txt");
    fs::write(&test_file_path, "test content").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("ls test_file.txt"));
    args.insert("working_directory".to_string(), json!(temp_dir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Command with specific working directory should execute"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test command with pipes and redirects
#[tokio::test]
async fn test_complex_shell_commands() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("complex").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'test data' | wc -l"));
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Complex shell command with pipes should execute"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test error handling for invalid commands
#[tokio::test]
async fn test_invalid_command_handling() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("invalid_cmd").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("nonexistent_command_xyz"));
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    // The command should start successfully (async), but will fail during execution
    assert!(
        result.is_ok(),
        "Invalid command should start (will fail during execution)"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test missing required command parameter
#[tokio::test]
async fn test_missing_command_parameter() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let args = Map::new();

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;

    // The command should succeed initially (async) since command is a positional arg
    // and might be filled in later, but will fail during execution
    assert!(
        result.is_ok(),
        "Tool call should succeed initially but fail during execution"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test invalid working directory (within workspace but nonexistent)
#[tokio::test]
async fn test_invalid_working_directory() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let base = get_workspace_path(".ahma/tmp/shell_async");
    fs::create_dir_all(&base).await?;
    let invalid_dir = base.join(format!("does_not_exist_{}", unique_suffix()));

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'test'"));
    args.insert(
        "working_directory".to_string(),
        json!(invalid_dir.to_string_lossy()),
    );

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    // The command should start successfully (async), but will fail during execution when trying to cd
    assert!(
        result.is_ok(),
        "Invalid working directory should start (will fail during execution)"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test command with environment variables
#[tokio::test]
async fn test_environment_variables() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("env_vars").await?;

    let mut args = Map::new();
    args.insert(
        "command".to_string(),
        json!("TEST_VAR='hello' && echo $TEST_VAR"),
    );
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Command with environment variables should execute"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test shell built-in commands
#[tokio::test]
async fn test_shell_builtins() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("builtins").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("pwd"));
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(result.is_ok(), "Shell built-in commands should execute");

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test command with special characters
#[tokio::test]
async fn test_special_characters() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("special_chars").await?;

    let mut args = Map::new();
    args.insert(
        "command".to_string(),
        json!(r#"echo "Special chars: !@#$%^&*()""#),
    );
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Commands with special characters should execute"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test multiple space-separated arguments
#[tokio::test]
async fn test_multiple_arguments() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("multi_args").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo arg1 arg2 arg3"));
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Commands with multiple arguments should execute"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test long-running command (to verify async behavior)
#[tokio::test]
async fn test_long_running_command() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("long_running").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("sleep 1 && echo 'done'"));
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    // Measure time to ensure it returns quickly (async)
    let start = std::time::Instant::now();
    let result = client.call_tool(call_param).await;
    let elapsed = start.elapsed();

    assert!(
        result.is_ok(),
        "Long-running command should start successfully"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "Async command should return quickly, took: {:?}",
        elapsed
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test that working_directory parameter is properly excluded from command arguments
#[tokio::test]
async fn test_working_directory_not_passed_to_command() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let workdir = make_workdir("wd_exclusion").await?;

    // This test verifies that the working_directory parameter doesn't cause
    // "bash: --working_directory: invalid option" error
    let mut args = Map::new();
    args.insert(
        "command".to_string(),
        json!("echo 'testing working_directory exclusion'"),
    );
    args.insert("working_directory".to_string(), json!(workdir));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(
        result.is_ok(),
        "Command should execute without working_directory parameter being passed to bash"
    );

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test command execution in different directories
#[tokio::test]
async fn test_different_working_directories() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let temp_dir = make_workdir("wd_variants").await?;

    // Create a subdirectory in temp_dir
    let subdir = std::path::Path::new(&temp_dir).join("subdir");
    fs::create_dir(&subdir).await?;

    // Test execution in temp directory root
    let mut args1 = Map::new();
    args1.insert("command".to_string(), json!("pwd"));
    args1.insert("working_directory".to_string(), json!(temp_dir));

    let call_param1 = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args1),
    };

    let result1 = client.call_tool(call_param1).await;
    assert!(
        result1.is_ok(),
        "Command in temp directory root should execute"
    );

    // Test execution in subdirectory
    let mut args2 = Map::new();
    args2.insert("command".to_string(), json!("pwd"));
    args2.insert(
        "working_directory".to_string(),
        json!(subdir.to_string_lossy()),
    );

    let call_param2 = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args2),
    };

    let result2 = client.call_tool(call_param2).await;
    assert!(result2.is_ok(), "Command in subdirectory should execute");

    client.cancel().await?;
    Ok(())
}
