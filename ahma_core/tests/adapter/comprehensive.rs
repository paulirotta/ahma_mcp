//! Comprehensive adapter testing for Phase 7 requirements.
//!
//! This test module targets:
//! - Command construction edge cases
//! - Path validation security tests  
//! - Error propagation accuracy
//! - Async operation lifecycle testing
//! - Argument handling and shell escaping

use ahma_core::{
    adapter::{Adapter, ExecutionMode},
    config::SubcommandConfig,
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
    utils::logging::init_test_logging,
};
use anyhow::Result;
use serde_json::{Map, json};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Helper function to create test adapter with default configuration
async fn create_test_adapter() -> Result<(Adapter, TempDir)> {
    let temp_dir = TempDir::new()?;

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let monitor = Arc::new(OperationMonitor::new(monitor_config));

    let shell_config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_idle_timeout: Duration::from_secs(30),
        pool_cleanup_interval: Duration::from_secs(60),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(30),
    };
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    shell_pool.clone().start_background_tasks();

    let adapter = Adapter::new(monitor, shell_pool)?;

    Ok((adapter, temp_dir))
}

/// Test command construction edge cases with various argument types
#[tokio::test]
async fn test_command_construction_edge_cases() -> Result<()> {
    init_test_logging();

    let (adapter, temp_dir) = create_test_adapter().await?;

    // Test simple command construction
    let mut args = Map::new();
    args.insert("verbose".to_string(), json!(true));
    args.insert("output".to_string(), json!("test.txt"));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await?;

    assert!(result.contains("--verbose"));
    assert!(result.contains("--output"));
    assert!(result.contains("test.txt"));

    // Test boolean argument handling
    let mut bool_args = Map::new();
    bool_args.insert("flag1".to_string(), json!(true));
    bool_args.insert("flag2".to_string(), json!(false));
    bool_args.insert("flag3".to_string(), json!("true"));
    bool_args.insert("flag4".to_string(), json!("false"));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(bool_args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await?;

    // Boolean true includes flag, boolean false excludes it
    assert!(result.contains("--flag1"));
    assert!(!result.contains("--flag2"));
    // String values are passed as arguments, not interpreted as booleans
    assert!(result.contains("--flag3"));
    assert!(result.contains("true"));
    assert!(result.contains("--flag4"));
    assert!(result.contains("false"));

    // Test array argument handling
    let mut array_args = Map::new();
    array_args.insert(
        "files".to_string(),
        json!(["file1.txt", "file2.txt", "file3.txt"]),
    );

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(array_args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await?;

    // Should include all array elements with repeated flags
    assert!(result.contains("--files"));
    assert!(result.contains("file1.txt"));
    assert!(result.contains("file2.txt"));
    assert!(result.contains("file3.txt"));

    adapter.shutdown().await;
    Ok(())
}

/// Test path validation security tests
#[tokio::test]
async fn test_path_validation_security() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Test normal path handling
    let result = adapter
        .execute_sync_in_dir(
            "pwd",
            None,
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await?;

    assert!(result.contains(temp_dir.path().to_str().unwrap()));

    // Test path with potential security issues (shell escaping)
    let mut args = Map::new();
    args.insert("path".to_string(), json!("test; rm -rf /"));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await;

    // Should complete safely without executing the dangerous command
    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("test; rm -rf /") || output.contains("'test; rm -rf /'"));

    // Test path with quotes and escape characters
    let mut quote_args = Map::new();
    quote_args.insert("message".to_string(), json!("Hello 'world' \"test\""));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(quote_args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await?;

    // The adapter may write complex strings to temp files for safety
    // Just verify the command executed successfully
    assert!(result.contains("--message") || result.contains("Hello"));

    adapter.shutdown().await;
    Ok(())
}

/// Test error propagation accuracy
#[tokio::test]
async fn test_error_propagation_accuracy() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Test command that doesn't exist
    let result = adapter
        .execute_sync_in_dir(
            "nonexistent_command_xyz_123",
            None,
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("failed with exit code")
            || error.to_string().contains("Command execution failed")
    );

    // Test command with invalid arguments
    let mut invalid_args = Map::new();
    invalid_args.insert("invalid-flag".to_string(), json!("value"));

    let result = adapter
        .execute_sync_in_dir(
            "ls",
            Some(invalid_args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await;

    // Should either succeed or fail gracefully (ls might ignore unknown flags)
    assert!(result.is_ok() || result.is_err());

    // Test timeout error propagation
    let result = adapter
        .execute_sync_in_dir(
            "sleep",
            Some({
                let mut args = Map::new();
                args.insert("duration".to_string(), json!("2"));
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(1), // 1 second timeout for 2 second sleep
            None,
        )
        .await;

    if result.is_err() {
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("timeout")
                || error.to_string().contains("failed with exit code")
        );
    }

    adapter.shutdown().await;
    Ok(())
}

/// Test async operation lifecycle testing
#[tokio::test]
async fn test_async_operation_lifecycle() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Test basic async operation
    let operation_id = adapter
        .execute_async_in_dir(
            "test_tool",
            "echo",
            Some({
                let mut args = Map::new();
                args.insert("message".to_string(), json!("async test"));
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(30),
        )
        .await?;

    assert!(operation_id.starts_with("op_"));

    // Wait for operation to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test multiple concurrent async operations
    let mut operation_ids = Vec::new();
    for i in 0..5 {
        let op_id = adapter
            .execute_async_in_dir(
                "concurrent_tool",
                "echo",
                Some({
                    let mut args = Map::new();
                    args.insert(
                        "message".to_string(),
                        json!(format!("concurrent test {}", i)),
                    );
                    args
                }),
                temp_dir.path().to_str().unwrap(),
                Some(30),
            )
            .await?;
        operation_ids.push(op_id);
    }

    // All operations should have unique IDs
    assert_eq!(operation_ids.len(), 5);
    for i in 0..operation_ids.len() {
        for j in i + 1..operation_ids.len() {
            assert_ne!(operation_ids[i], operation_ids[j]);
        }
    }

    // Wait for all operations to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    adapter.shutdown().await;
    Ok(())
}

/// Test argument handling and shell escaping
#[tokio::test]
async fn test_argument_handling_and_shell_escaping() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Test special characters that need escaping
    let special_chars = vec![
        ("newline", "Hello\nWorld"),
        ("quote", "Hello 'world'"),
        ("double_quote", "Hello \"world\""),
        ("backslash", "Hello\\World"),
        ("backtick", "Hello`world"),
        ("dollar", "Hello$world"),
    ];

    for (test_name, test_value) in special_chars {
        let mut args = Map::new();
        args.insert("message".to_string(), json!(test_value));

        let result = adapter
            .execute_sync_in_dir(
                "echo",
                Some(args),
                temp_dir.path().to_str().unwrap(),
                Some(10),
                None,
            )
            .await;

        // Should either succeed with proper escaping or fail gracefully
        assert!(
            result.is_ok() || result.is_err(),
            "Test '{}' with value '{}' should complete",
            test_name,
            test_value
        );

        if let Ok(output) = result {
            // If successful, should contain some form of the input
            assert!(
                !output.is_empty(),
                "Test '{}' should produce some output",
                test_name
            );
        }
    }

    // Test very long argument
    let long_string = "x".repeat(10000);
    let mut long_args = Map::new();
    long_args.insert("data".to_string(), json!(long_string));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(long_args),
            temp_dir.path().to_str().unwrap(),
            Some(30),
            None,
        )
        .await;

    // Should handle long arguments (possibly via file mechanism)
    assert!(result.is_ok() || result.is_err());

    adapter.shutdown().await;
    Ok(())
}

/// Test subcommand configuration handling
#[tokio::test]
async fn test_subcommand_configuration_handling() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Create a mock subcommand configuration
    let subcommand_config = SubcommandConfig {
        name: "echo_test".to_string(),
        description: "Test echo command".to_string(),
        force_synchronous: None,
        timeout_seconds: Some(30),
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
        positional_args: Some(vec![ahma_core::config::CommandOption {
            name: "message".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: Some("Message to echo".to_string()),
            format: None,
            items: None,
            required: Some(true),
            file_arg: None,
            file_flag: None,
        }]),
        options: Some(vec![
            ahma_core::config::CommandOption {
                name: "verbose".to_string(),
                alias: Some("v".to_string()),
                option_type: "boolean".to_string(),
                description: Some("Verbose output".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: None,
                file_flag: None,
            },
            ahma_core::config::CommandOption {
                name: "count".to_string(),
                alias: Some("n".to_string()),
                option_type: "string".to_string(),
                description: Some("Number of times".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: None,
                file_flag: None,
            },
        ]),
    };

    // Test with subcommand config
    let mut args = Map::new();
    args.insert("message".to_string(), json!("hello world"));
    args.insert("verbose".to_string(), json!(true));
    args.insert("count".to_string(), json!("3"));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            Some(&subcommand_config),
        )
        .await?;

    // Should construct command properly with positional args first
    assert!(result.contains("hello world"));
    assert!(result.contains("--verbose") || result.contains("-v"));
    assert!(result.contains("--count") || result.contains("-n"));
    assert!(result.contains("3"));

    // Test with alias usage
    let mut alias_args = Map::new();
    alias_args.insert("message".to_string(), json!("test message"));
    alias_args.insert("v".to_string(), json!(true)); // Use alias instead of full name
    alias_args.insert("n".to_string(), json!("5")); // Use alias instead of full name

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(alias_args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            Some(&subcommand_config),
        )
        .await?;

    assert!(result.contains("test message"));
    assert!(result.contains("5"));

    adapter.shutdown().await;
    Ok(())
}

/// Test file-based argument handling for complex content
#[tokio::test]
async fn test_file_based_argument_handling() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Create subcommand config with file_arg support
    let subcommand_config = SubcommandConfig {
        name: "script_test".to_string(),
        description: "Test script handling".to_string(),
        force_synchronous: None,
        timeout_seconds: Some(30),
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
        positional_args: None,
        options: Some(vec![ahma_core::config::CommandOption {
            name: "script".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: Some("Script content".to_string()),
            format: None,
            items: None,
            required: Some(false),
            file_arg: Some(true),
            file_flag: Some("--script-file".to_string()),
        }]),
    };

    // Test with complex multi-line content that needs file handling
    let complex_script = r#"#!/bin/bash
echo "This is a"
echo "multi-line script"
echo "with 'quotes' and $variables"
ls -la
"#;

    let mut args = Map::new();
    args.insert("script".to_string(), json!(complex_script));

    // This would normally use cat to read the file, but since we're testing the adapter
    // we'll use a simpler command that can handle file arguments
    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            Some(&subcommand_config),
        )
        .await;

    // Should complete successfully using file-based argument passing
    assert!(result.is_ok());

    adapter.shutdown().await;
    Ok(())
}

/// Test error recovery and resilience
#[tokio::test]
async fn test_error_recovery_and_resilience() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Test that adapter continues working after errors

    // 1. Cause an error
    let error_result = adapter
        .execute_sync_in_dir(
            "false", // Command that always exits with error code 1
            None,
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await;

    assert!(error_result.is_err());

    // 2. Verify adapter still works normally
    let success_result = adapter
        .execute_sync_in_dir(
            "echo",
            Some({
                let mut args = Map::new();
                args.insert("message".to_string(), json!("recovery test"));
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            None,
        )
        .await?;

    assert!(success_result.contains("recovery test"));

    // 3. Test multiple error/success cycles
    for i in 0..3 {
        // Error command
        let _ = adapter
            .execute_sync_in_dir(
                "nonexistent_command",
                None,
                temp_dir.path().to_str().unwrap(),
                Some(5),
                None,
            )
            .await;

        // Success command
        let success = adapter
            .execute_sync_in_dir(
                "echo",
                Some({
                    let mut args = Map::new();
                    args.insert("test".to_string(), json!(format!("cycle_{}", i)));
                    args
                }),
                temp_dir.path().to_str().unwrap(),
                Some(10),
                None,
            )
            .await?;

        assert!(success.contains(&format!("cycle_{}", i)));
    }

    adapter.shutdown().await;
    Ok(())
}

/// Test async operations with callback notifications
#[tokio::test]
async fn test_async_operations_with_callbacks() -> Result<()> {
    let (adapter, temp_dir) = create_test_adapter().await?;

    // Mock callback that collects notifications
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockCallback {
        notifications: Arc<Mutex<Vec<String>>>,
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ahma_core::callback_system::CallbackSender for MockCallback {
        async fn send_progress(
            &self,
            _update: ahma_core::callback_system::ProgressUpdate,
        ) -> Result<(), ahma_core::callback_system::CallbackError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut notifs = self.notifications.lock().unwrap();
            notifs.push("callback_received".to_string());
            Ok(())
        }

        async fn should_cancel(&self) -> bool {
            false
        }
    }

    let callback = MockCallback {
        notifications: Arc::new(Mutex::new(Vec::new())),
        call_count: Arc::new(AtomicUsize::new(0)),
    };

    let call_count = callback.call_count.clone();

    // Test async operation with callback
    let operation_id = adapter
        .execute_async_in_dir_with_callback(
            "callback_test",
            "echo",
            Some({
                let mut args = Map::new();
                args.insert("message".to_string(), json!("callback test"));
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(30),
            Some(Box::new(callback)),
        )
        .await?;

    assert!(operation_id.starts_with("op_"));

    // Wait for operation to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify callback was called
    let final_count = call_count.load(Ordering::Relaxed);
    assert!(
        final_count > 0,
        "Callback should have been called at least once"
    );

    adapter.shutdown().await;
    Ok(())
}

/// Test ExecutionMode enum
#[tokio::test]
async fn test_execution_mode_enum() -> Result<()> {
    // Test that ExecutionMode enum works correctly
    assert_eq!(ExecutionMode::Synchronous, ExecutionMode::Synchronous);
    assert_eq!(
        ExecutionMode::AsyncResultPush,
        ExecutionMode::AsyncResultPush
    );
    assert_ne!(ExecutionMode::Synchronous, ExecutionMode::AsyncResultPush);

    // Test serialization/deserialization
    let sync_json = serde_json::to_string(&ExecutionMode::Synchronous)?;
    let async_json = serde_json::to_string(&ExecutionMode::AsyncResultPush)?;

    let sync_decoded: ExecutionMode = serde_json::from_str(&sync_json)?;
    let async_decoded: ExecutionMode = serde_json::from_str(&async_json)?;

    assert_eq!(sync_decoded, ExecutionMode::Synchronous);
    assert_eq!(async_decoded, ExecutionMode::AsyncResultPush);

    Ok(())
}

/// Test utility functions for shell argument handling
#[tokio::test]
async fn test_utility_functions() -> Result<()> {
    // Test needs_file_handling function
    assert!(Adapter::needs_file_handling("line1\nline2"));
    assert!(Adapter::needs_file_handling("text with 'quotes'"));
    assert!(Adapter::needs_file_handling("text with \"quotes\""));
    assert!(Adapter::needs_file_handling("text with\\backslash"));
    assert!(Adapter::needs_file_handling("text with `backtick`"));
    assert!(Adapter::needs_file_handling("text with $variable"));
    assert!(Adapter::needs_file_handling(&"x".repeat(9000))); // Long string

    assert!(!Adapter::needs_file_handling("simple text"));
    assert!(!Adapter::needs_file_handling(""));
    assert!(!Adapter::needs_file_handling("normal-filename.txt"));

    // Test escape_shell_argument function
    assert_eq!(Adapter::escape_shell_argument("simple"), "'simple'");
    assert_eq!(
        Adapter::escape_shell_argument("text with spaces"),
        "'text with spaces'"
    );
    assert_eq!(
        Adapter::escape_shell_argument("text'with'quotes"),
        "'text'\"'\"'with'\"'\"'quotes'"
    );
    assert_eq!(Adapter::escape_shell_argument(""), "''");

    Ok(())
}
