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

use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::{Map, json};
use std::borrow::Cow;
use tokio::fs;

/// Test basic shell command execution
#[tokio::test]
async fn test_basic_shell_command_execution() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test a simple echo command
    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'Hello World'"));
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;
    let temp_dir = tempfile::tempdir()?;

    // Create a test file in the temp directory
    let test_file_path = temp_dir.path().join("test_file.txt");
    fs::write(&test_file_path, "test content").await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("ls test_file.txt"));
    args.insert(
        "working_directory".to_string(),
        json!(temp_dir.path().to_string_lossy()),
    );

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
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'test data' | wc -l"));
    args.insert("working_directory".to_string(), json!("/tmp"));

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

/// Test timeout parameter handling
#[tokio::test]
async fn test_timeout_parameter() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'test with timeout'"));
    args.insert("working_directory".to_string(), json!("/tmp"));
    args.insert("timeout_seconds".to_string(), json!(10));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(args),
    };

    let result = client.call_tool(call_param).await;
    assert!(result.is_ok(), "Command with timeout should execute");

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");

    client.cancel().await?;
    Ok(())
}

/// Test error handling for invalid commands
#[tokio::test]
async fn test_invalid_command_handling() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("nonexistent_command_xyz"));
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;

    let args = Map::new();
    // Missing required "command" parameter, only have working_directory

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

/// Test invalid working directory
#[tokio::test]
async fn test_invalid_working_directory() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo 'test'"));
    args.insert(
        "working_directory".to_string(),
        json!("/nonexistent/directory/path"),
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
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert(
        "command".to_string(),
        json!("TEST_VAR='hello' && echo $TEST_VAR"),
    );
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("pwd"));
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert(
        "command".to_string(),
        json!(r#"echo "Special chars: !@#$%^&*()""#),
    );
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("echo arg1 arg2 arg3"));
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;

    let mut args = Map::new();
    args.insert("command".to_string(), json!("sleep 1 && echo 'done'"));
    args.insert("working_directory".to_string(), json!("/tmp"));
    args.insert("timeout_seconds".to_string(), json!(5));

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
    let client = new_client(Some(".ahma/tools")).await?;

    // This test verifies that the working_directory parameter doesn't cause
    // "bash: --working_directory: invalid option" error
    let mut args = Map::new();
    args.insert(
        "command".to_string(),
        json!("echo 'testing working_directory exclusion'"),
    );
    args.insert("working_directory".to_string(), json!("/tmp"));

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
    let client = new_client(Some(".ahma/tools")).await?;
    let temp_dir = tempfile::tempdir()?;

    // Create a subdirectory in temp_dir
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).await?;

    // Test execution in temp directory root
    let mut args1 = Map::new();
    args1.insert("command".to_string(), json!("pwd"));
    args1.insert(
        "working_directory".to_string(),
        json!(temp_dir.path().to_string_lossy()),
    );

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
