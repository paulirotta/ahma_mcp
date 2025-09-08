/// Test multi-line argument handling functionality
///
/// This test verifies that the adapter correctly handles multi-line strings and special characters
/// in command arguments by either using file-based argument passing or safe shell escaping.
use ahma_mcp::{
    adapter::{Adapter, AsyncExecOptions},
    config::{OptionConfig, SubcommandConfig},
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tempfile::tempdir;

#[tokio::test]
async fn test_simple_git_commit_without_multiline() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    // Initialize a git repository for testing
    let init_result = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git init");

    assert!(init_result.status.success(), "Git init failed");

    // Configure git user for testing
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to configure git user email");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to configure git user name");

    // Create a test file to commit
    std::fs::write(repo_path.join("test.txt"), "test content").expect("Failed to create test file");

    // Add the file to git
    let add_result = std::process::Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git add");

    assert!(add_result.status.success(), "Git add failed");

    // Test git commit directly to make sure it works
    let direct_commit = std::process::Command::new("git")
        .args(["commit", "-m", "test commit"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git commit directly");

    if !direct_commit.status.success() {
        eprintln!(
            "Direct commit stderr: {}",
            String::from_utf8_lossy(&direct_commit.stderr)
        );
        eprintln!(
            "Direct commit stdout: {}",
            String::from_utf8_lossy(&direct_commit.stdout)
        );
        panic!("Direct git commit failed");
    }

    println!("Direct git commit succeeded!");
}

#[tokio::test]
async fn test_multiline_argument_with_echo() {
    let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
        Duration::from_secs(30),
    )));
    let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));
    let adapter = Adapter::new(monitor.clone(), shell_pool).expect("Failed to create adapter");

    // Create a config for echo that supports file arguments
    let echo_config = SubcommandConfig {
        name: "default".to_string(),
        description: "Echo command".to_string(),
        options: None,
        positional_args: Some(vec![OptionConfig {
            name: "text".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: "Text to echo".to_string(),
            format: None,
            required: Some(true),
            positional: Some(true),
            file_arg: None, // Echo doesn't support files, should use escaping
            file_flag: None,
        }]),
        synchronous: Some(true),
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
    };

    // Test with a multi-line string that should trigger escaping
    let multiline_text = "Line 1\nLine 2\nLine 3";

    let args = json!({
        "text": multiline_text
    });

    let result = adapter
        .execute_async_in_dir_with_options(
            "echo_test",
            "echo",
            "/tmp",
            AsyncExecOptions {
                operation_id: Some("test_echo_multiline".to_string()),
                args: args.as_object().cloned(),
                timeout: Some(10),
                callback: None,
                subcommand_config: Some(&echo_config),
            },
        )
        .await;

    // Wait for the operation to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check the operation result through the monitor
    let operation = monitor.get_operation("test_echo_multiline").await;
    println!("Operation result: {:?}", operation);

    // The command should succeed (proper escaping prevents shell injection)
    assert!(
        result.is_ok(),
        "Echo with multi-line text failed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_multiline_git_commit_with_real_tool() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    // Initialize a git repository for testing
    let init_result = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git init");

    assert!(init_result.status.success(), "Git init failed");

    // Configure git user for testing
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to configure git user email");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to configure git user name");

    // Create a test file to commit
    std::fs::write(repo_path.join("test.txt"), "test content").expect("Failed to create test file");

    // Add the file to git
    let add_result = std::process::Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git add");

    assert!(add_result.status.success(), "Git add failed");

    // Setup the adapter
    let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
        Duration::from_secs(30),
    )));
    let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));
    let adapter = Adapter::new(monitor.clone(), shell_pool).expect("Failed to create adapter");

    // Load the real git tool configuration
    let git_tool_path = ".ahma/tools/git.json";
    let git_config_str =
        std::fs::read_to_string(git_tool_path).expect("Failed to read git tool config");
    let git_tool: ahma_mcp::config::ToolConfig =
        serde_json::from_str(&git_config_str).expect("Failed to parse git tool config");

    // Find the commit subcommand
    let commit_subcommand = git_tool
        .subcommand
        .as_ref()
        .and_then(|subcommands| subcommands.iter().find(|s| s.name == "commit"))
        .expect("Failed to find commit subcommand in git tool config");

    // Test with a multi-line commit message that should trigger file-based handling
    let multiline_message = "feat: implement multi-line argument handling\n\nThis commit adds support for:\n- Automatic detection of problematic arguments\n- File-based argument passing for tools that support it\n- Safe shell escaping as fallback\n\nTested with git commit -F functionality.";

    let args = json!({
        "message": multiline_message
    });

    println!("Testing git commit with multi-line message through real tool config...");

    // Execute using the adapter with the real commit subcommand config
    let result = adapter
        .execute_async_in_dir_with_options(
            "git_commit",
            "git commit",
            repo_path.to_str().unwrap(),
            AsyncExecOptions {
                operation_id: Some("test_multiline_commit_real".to_string()),
                args: args.as_object().cloned(),
                timeout: Some(30),
                callback: None,
                subcommand_config: Some(commit_subcommand),
            },
        )
        .await;

    println!("Git commit result: {:?}", result);

    // Verify the command was executed successfully
    assert!(
        result.is_ok(),
        "Git commit with multi-line message failed: {:?}",
        result
    );

    // Wait a moment for the operation to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify the commit was actually created
    let log_result = std::process::Command::new("git")
        .args(["log", "--oneline", "-1"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git log");

    if !log_result.status.success() {
        eprintln!(
            "Git log stderr: {}",
            String::from_utf8_lossy(&log_result.stderr)
        );
        eprintln!(
            "Git log stdout: {}",
            String::from_utf8_lossy(&log_result.stdout)
        );

        // Debug: Check if there are any files staged
        let status_result = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to run git status");
        eprintln!(
            "Git status after commit attempt: {}",
            String::from_utf8_lossy(&status_result.stdout)
        );

        panic!("Git log failed - commit was not created");
    }
    let log_output = String::from_utf8_lossy(&log_result.stdout);
    assert!(
        log_output.contains("implement multi-line argument handling"),
        "Commit message not found in git log: {}",
        log_output
    );
}

#[tokio::test]
async fn test_multiline_git_commit_message() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    // Initialize a git repository for testing
    let init_result = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git init");

    assert!(init_result.status.success(), "Git init failed");

    // Configure git user for testing
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to configure git user email");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to configure git user name");

    // Create a test file to commit
    std::fs::write(repo_path.join("test.txt"), "test content").expect("Failed to create test file");

    // Add the file to git
    let add_result = std::process::Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git add");

    assert!(add_result.status.success(), "Git add failed");

    // Setup the adapter
    let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
        Duration::from_secs(30),
    )));
    let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));
    let adapter = Adapter::new(monitor.clone(), shell_pool).expect("Failed to create adapter");

    // Create a config for git commit with file_arg support
    let commit_config = SubcommandConfig {
        name: "commit".to_string(),
        description: "Record changes to the repository".to_string(),
        options: Some(vec![OptionConfig {
            name: "message".to_string(),
            alias: Some("m".to_string()),
            option_type: "string".to_string(),
            description: "Use the given message as the commit message".to_string(),
            format: None,
            required: None,
            positional: None,
            file_arg: Some(true),
            file_flag: Some("-F".to_string()),
        }]),
        positional_args: None,
        synchronous: Some(false), // Changed to async for proper operation tracking
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
    };

    // Test with a multi-line commit message that should trigger file-based handling
    let multiline_message = "feat: implement multi-line argument handling\n\nThis commit adds support for:\n- Automatic detection of problematic arguments\n- File-based argument passing for tools that support it\n- Safe shell escaping as fallback\n\nTested with git commit -F functionality.";

    let args = json!({
        "message": multiline_message
    });

    // Execute the git commit command
    let result = adapter
        .execute_async_in_dir_with_options(
            "git_commit",
            "git commit", // Use full command like the working test
            repo_path.to_str().unwrap(),
            AsyncExecOptions {
                operation_id: Some("test_multiline_commit".to_string()),
                args: args.as_object().cloned(),
                timeout: Some(30),
                callback: None,
                subcommand_config: Some(&commit_config),
            },
        )
        .await;

    // Verify the command was executed successfully
    if let Err(e) = &result {
        eprintln!("Git commit failed with error: {:?}", e);
    }
    println!("Git commit result: {:?}", result);

    // Check what operations exist in the monitor
    tokio::time::sleep(Duration::from_millis(100)).await;
    let all_operations = monitor.get_all_operations().await;
    println!("All operations after execution: {:?}", all_operations);

    assert!(
        result.is_ok(),
        "Git commit with multi-line message failed: {:?}",
        result
    );

    // Wait for the operation to complete
    let mut wait_time = 0;
    let mut completed = false;
    while wait_time < 50 && !completed {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(operation) = monitor.get_operation("test_multiline_commit").await {
            println!("Operation status: {:?}", operation.state);
            if matches!(
                operation.state,
                ahma_mcp::operation_monitor::OperationStatus::Completed
                    | ahma_mcp::operation_monitor::OperationStatus::Failed
            ) {
                completed = true;
                println!("Operation completed with status: {:?}", operation.state);
                if let Some(result_data) = &operation.result {
                    println!("Operation result: {:?}", result_data);
                }
                if matches!(
                    operation.state,
                    ahma_mcp::operation_monitor::OperationStatus::Failed
                ) {
                    eprintln!("Operation failed: {:?}", operation);
                }
            }
        }
        wait_time += 1;
    }

    // Debug: Check if there are any files staged
    let status_result = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git status");
    eprintln!(
        "Git status after commit attempt: {}",
        String::from_utf8_lossy(&status_result.stdout)
    );

    // Verify the commit was actually created
    let log_result = std::process::Command::new("git")
        .args(["log", "--oneline", "-1"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git log");

    if !log_result.status.success() {
        eprintln!(
            "Git log stderr: {}",
            String::from_utf8_lossy(&log_result.stderr)
        );
        eprintln!(
            "Git log stdout: {}",
            String::from_utf8_lossy(&log_result.stdout)
        );
        panic!("Git log failed");
    }
    let log_output = String::from_utf8_lossy(&log_result.stdout);
    assert!(
        log_output.contains("implement multi-line argument handling"),
        "Commit message not found in git log: {}",
        log_output
    );
}

#[tokio::test]
async fn test_special_characters_in_arguments() {
    let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
        Duration::from_secs(30),
    )));
    let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));
    let adapter = Adapter::new(monitor, shell_pool).expect("Failed to create adapter");

    // Test with a simple echo command that handles special characters
    let echo_config = SubcommandConfig {
        name: "echo".to_string(),
        description: "Display a line of text".to_string(),
        options: None,
        positional_args: Some(vec![OptionConfig {
            name: "text".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: "Text to display".to_string(),
            format: None,
            required: Some(true),
            positional: Some(true),
            file_arg: None, // No file support, should use escaping
            file_flag: None,
        }]),
        synchronous: Some(true),
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
    };

    // Test with text containing special characters
    let special_text = "Hello 'world' with \"quotes\" and $variables and `backticks`";

    let args = json!({
        "text": special_text
    });

    let result = adapter
        .execute_async_in_dir_with_options(
            "echo_test",
            "echo",
            "/tmp",
            AsyncExecOptions {
                operation_id: Some("test_special_chars".to_string()),
                args: args.as_object().cloned(),
                timeout: Some(10),
                callback: None,
                subcommand_config: Some(&echo_config),
            },
        )
        .await;

    // The command should succeed (proper escaping prevents shell injection)
    assert!(
        result.is_ok(),
        "Echo with special characters failed: {:?}",
        result
    );
}

#[test]
fn test_needs_file_handling_detection() {
    // Test the static method for detecting problematic strings
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        "line1\nline2"
    ));
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        "text with 'single quotes'"
    ));
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        "text with \"double quotes\""
    ));
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        "text with $variables"
    ));
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        "text with `backticks`"
    ));
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        "text with \\backslashes"
    ));

    // Very long strings should also use file handling
    let long_string = "a".repeat(10000);
    assert!(ahma_mcp::adapter::Adapter::needs_file_handling(
        &long_string
    ));

    // Normal strings should not need file handling
    assert!(!ahma_mcp::adapter::Adapter::needs_file_handling(
        "simple text"
    ));
    assert!(!ahma_mcp::adapter::Adapter::needs_file_handling(
        "text with spaces"
    ));
    assert!(!ahma_mcp::adapter::Adapter::needs_file_handling(
        "text-with-dashes"
    ));
}

#[test]
fn test_shell_argument_escaping() {
    // Test the shell escaping functionality
    assert_eq!(
        ahma_mcp::adapter::Adapter::escape_shell_argument("simple"),
        "'simple'"
    );

    assert_eq!(
        ahma_mcp::adapter::Adapter::escape_shell_argument("text with spaces"),
        "'text with spaces'"
    );

    // Test escaping of embedded single quotes
    assert_eq!(
        ahma_mcp::adapter::Adapter::escape_shell_argument("don't break"),
        "'don'\"'\"'t break'"
    );

    assert_eq!(
        ahma_mcp::adapter::Adapter::escape_shell_argument("it's a 'test'"),
        "'it'\"'\"'s a '\"'\"'test'\"'\"''"
    );
}
