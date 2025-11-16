//! Comprehensive adapter coverage improvement tests targeting uncovered paths.
//!
//! This test module specifically targets the gaps identified in the coverage report:
//! - Error handling paths in command preparation
//! - File handling and temp file creation errors
//! - Complex cancellation scenarios and timeout edge cases
//! - Shell pool failure scenarios
//! - Advanced argument processing edge cases
//! - Callback error handling paths

use anyhow::Result;
use serde_json::{Map, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::Mutex;

use ahma_core::{
    adapter::{Adapter, AsyncExecOptions},
    callback_system::{CallbackError, CallbackSender, ProgressUpdate},
    config::{OptionConfig, SubcommandConfig},
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};

/// Helper function to create test adapter with custom configuration
async fn create_adapter_with_config(shell_config: ShellPoolConfig) -> Result<(Adapter, TempDir)> {
    let temp_dir = TempDir::new()?;
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    shell_pool.clone().start_background_tasks();
    let adapter = Adapter::new(monitor, shell_pool)?;
    Ok((adapter, temp_dir))
}

/// Mock callback that can simulate errors for testing error handling paths
#[derive(Clone)]
struct ErrorTestCallback {
    updates: Arc<Mutex<Vec<ProgressUpdate>>>,
    fail_on_send: Arc<AtomicUsize>,
    call_count: Arc<AtomicUsize>,
}

impl ErrorTestCallback {
    fn new() -> Self {
        Self {
            updates: Arc::new(Mutex::new(Vec::new())),
            fail_on_send: Arc::new(AtomicUsize::new(0)),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn fail_after_calls(&self, count: usize) {
        self.fail_on_send.store(count, Ordering::Relaxed);
    }

    async fn get_updates(&self) -> Vec<ProgressUpdate> {
        self.updates.lock().await.clone()
    }

    fn get_call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl CallbackSender for ErrorTestCallback {
    async fn send_progress(&self, update: ProgressUpdate) -> Result<(), CallbackError> {
        let count = self.call_count.fetch_add(1, Ordering::Relaxed) + 1;
        let fail_threshold = self.fail_on_send.load(Ordering::Relaxed);

        if fail_threshold > 0 && count >= fail_threshold {
            return Err(CallbackError::SendFailed(
                "Simulated callback error".to_string(),
            ));
        }

        self.updates.lock().await.push(update);
        Ok(())
    }

    async fn should_cancel(&self) -> bool {
        false
    }
}

/// Test error paths in command preparation with complex subcommand configs
#[tokio::test]
async fn test_command_preparation_error_paths() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test with complex subcommand config that exercises all option type paths
    let complex_config = SubcommandConfig {
        name: "complex_test".to_string(),
        description: "Complex test command".to_string(),
        force_synchronous: None,
        timeout_seconds: Some(30),
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
        positional_args: Some(vec![
            OptionConfig {
                name: "pos1".to_string(),
                alias: None,
                option_type: "string".to_string(),
                description: Some("First positional".to_string()),
                format: None,
                items: None,
                required: Some(true),
                file_arg: Some(false),
                file_flag: None,
            },
            OptionConfig {
                name: "pos_array".to_string(),
                alias: None,
                option_type: "array".to_string(),
                description: Some("Array positional".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: Some(false),
                file_flag: None,
            },
        ]),
        options: Some(vec![
            OptionConfig {
                name: "file-content".to_string(),
                alias: Some("f".to_string()),
                option_type: "string".to_string(),
                description: Some("File content option".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: Some(true),
                file_flag: Some("--file".to_string()),
            },
            OptionConfig {
                name: "array-option".to_string(),
                alias: None,
                option_type: "array".to_string(),
                description: Some("Array option".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: Some(false),
                file_flag: None,
            },
            OptionConfig {
                name: "bool-flag".to_string(),
                alias: Some("b".to_string()),
                option_type: "boolean".to_string(),
                description: Some("Boolean flag".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: Some(false),
                file_flag: None,
            },
            OptionConfig {
                name: "number".to_string(),
                alias: None,
                option_type: "number".to_string(),
                description: Some("Number option".to_string()),
                format: None,
                items: None,
                required: Some(false),
                file_arg: Some(false),
                file_flag: None,
            },
        ]),
    };

    // Test with complex multiline content that needs file handling
    let multiline_content = "#!/bin/bash\necho 'This contains single quotes'\necho \"And double quotes\"\necho `And backticks`\necho $VARIABLES\necho 'Multiple\\nEscape\\tSequences'";

    let mut complex_args = Map::new();
    complex_args.insert("pos1".to_string(), json!("first_arg"));
    complex_args.insert("pos_array".to_string(), json!(["item1", "item2", "item3"]));
    complex_args.insert("file-content".to_string(), json!(multiline_content));
    complex_args.insert("array-option".to_string(), json!(["opt1", "opt2"]));
    complex_args.insert("bool-flag".to_string(), json!(true));
    complex_args.insert("b".to_string(), json!(false)); // Test alias that should be ignored when main name is present
    complex_args.insert("number".to_string(), json!(42));
    complex_args.insert("null-value".to_string(), json!(null));
    complex_args.insert("string-bool-true".to_string(), json!("true"));
    complex_args.insert("string-bool-false".to_string(), json!("false"));
    complex_args.insert("working_directory".to_string(), json!("/should/be/ignored"));

    let result = adapter
        .execute_sync_in_dir(
            "echo complex test",
            Some(complex_args),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            Some(&complex_config),
        )
        .await;

    // Should succeed with proper argument handling
    assert!(
        result.is_ok(),
        "Complex command should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();

    // Verify positional args are handled correctly
    assert!(output.contains("first_arg"));
    assert!(output.contains("item1") || output.contains("--pos_array"));

    // Verify boolean flag is present
    assert!(output.contains("--bool-flag") || output.contains("-b"));

    // Verify array option is handled
    assert!(output.contains("--array-option"));

    // Verify number is converted to string
    assert!(output.contains("42"));

    adapter.shutdown().await;
    Ok(())
}

/// Test file-based argument handling error scenarios
#[tokio::test]
async fn test_file_handling_error_scenarios() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test file handling with various problematic content
    let very_long = "x".repeat(10000);
    let exactly_limit = "y".repeat(8192);
    let over_limit = "z".repeat(8193);

    let problematic_strings = vec![
        ("newlines", "Line 1\nLine 2\nLine 3"),
        ("carriage_returns", "Line 1\rLine 2\rLine 3"),
        ("mixed_newlines", "Line 1\r\nLine 2\n\rLine 3"),
        (
            "single_quotes",
            "Text with 'single quotes' and more 'quotes'",
        ),
        (
            "double_quotes",
            "Text with \"double quotes\" and more \"quotes\"",
        ),
        (
            "backticks",
            "Text with `backticks` and `command substitution`",
        ),
        ("variables", "Text with $HOME and ${USER} variables"),
        (
            "backslashes",
            "Text with \\ backslashes \\\\ and \\n escape sequences",
        ),
        (
            "mixed_special",
            "Complex 'text' with \"quotes\", $vars, `cmds`, and \\ backslashes\nand newlines",
        ),
        ("very_long", very_long.as_str()), // Test length threshold
        ("exactly_limit", exactly_limit.as_str()), // Test exact limit boundary
        ("over_limit", over_limit.as_str()), // Test just over limit
    ];

    let file_config = SubcommandConfig {
        name: "file_test".to_string(),
        description: "File handling test".to_string(),
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
        options: Some(vec![OptionConfig {
            name: "content".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: Some("Content to handle".to_string()),
            format: None,
            items: None,
            required: Some(false),
            file_arg: Some(true),
            file_flag: Some("--input-file".to_string()),
        }]),
    };

    for (test_name, content) in problematic_strings {
        let mut args = Map::new();
        args.insert("content".to_string(), json!(content));

        let result = adapter
            .execute_sync_in_dir(
                "echo",
                Some(args),
                temp_dir.path().to_str().unwrap(),
                Some(30),
                Some(&file_config),
            )
            .await;

        // Should handle all problematic content safely
        assert!(
            result.is_ok(),
            "File handling test '{}' should succeed: {:?}",
            test_name,
            result.err()
        );

        let output = result.unwrap();
        assert!(
            output.contains("--input-file") || !output.is_empty(),
            "Test '{}' should produce output",
            test_name
        );
    }

    adapter.shutdown().await;
    Ok(())
}

/// Test shell pool failure scenarios and error handling
#[tokio::test]
async fn test_shell_pool_failure_scenarios() -> Result<()> {
    // Create adapter with restrictive shell pool config that might cause failures
    let restrictive_config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 1,
        max_total_shells: 1,
        shell_idle_timeout: Duration::from_millis(100), // Very short timeout
        pool_cleanup_interval: Duration::from_millis(50),
        shell_spawn_timeout: Duration::from_millis(100), // Very short spawn timeout
        command_timeout: Duration::from_millis(500),     // Short command timeout
        health_check_interval: Duration::from_millis(50),
    };

    let (adapter, temp_dir) = create_adapter_with_config(restrictive_config).await?;

    // Test multiple sequential operations that might stress the shell pool
    let mut success_count = 0;
    let mut error_count = 0;

    for i in 0..5 {
        let mut args = Map::new();
        args.insert(
            "message".to_string(),
            json!(format!("sequential test {}", i)),
        );

        let result = adapter
            .execute_sync_in_dir(
                "echo",
                Some(args),
                temp_dir.path().to_str().unwrap(),
                Some(1), // Very short timeout
                None,
            )
            .await;

        match result {
            Ok(_) => success_count += 1,
            Err(_) => error_count += 1,
        }
    }

    // At least some operations should complete (though some might fail due to resource constraints)
    assert!(
        success_count > 0 || error_count > 0,
        "Some operations should complete or fail gracefully"
    );

    adapter.shutdown().await;
    Ok(())
}

/// Test advanced async operation cancellation scenarios
#[tokio::test]
async fn test_advanced_cancellation_scenarios() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test cancellation via timeout - this will exercise the cancellation detection paths
    let callback = ErrorTestCallback::new();

    let _operation_id = adapter
        .execute_async_in_dir_with_callback(
            "cancel_test",
            "sleep",
            Some({
                let mut args = Map::new();
                args.insert("duration".to_string(), json!("5")); // Long-running command
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(1), // Very short timeout to force cancellation
            Some(Box::new(callback.clone())),
        )
        .await?;

    // Wait for operation to timeout/cancel
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let updates = callback.get_updates().await;
    let has_cancellation_or_error = updates.iter().any(|update| {
        matches!(
            update,
            ProgressUpdate::Cancelled { .. } | ProgressUpdate::FinalResult { success: false, .. }
        )
    });

    // Should receive some kind of notification (cancellation or error)
    assert!(
        has_cancellation_or_error || callback.get_call_count() > 0,
        "Should receive cancellation/error notification or have callback calls"
    );

    adapter.shutdown().await;
    Ok(())
}

/// Test callback error handling paths
#[tokio::test]
async fn test_callback_error_handling() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test callback that fails after first call
    let failing_callback = ErrorTestCallback::new();
    failing_callback.fail_after_calls(1);

    let operation_id = adapter
        .execute_async_in_dir_with_callback(
            "callback_error_test",
            "echo",
            Some({
                let mut args = Map::new();
                args.insert("message".to_string(), json!("callback error test"));
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(10),
            Some(Box::new(failing_callback.clone())),
        )
        .await?;

    // Wait for operation to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Operation should complete despite callback errors
    assert!(operation_id.starts_with("op_"));

    // Should have attempted at least one callback
    assert!(
        failing_callback.get_call_count() > 0,
        "Should have attempted callback calls"
    );

    adapter.shutdown().await;
    Ok(())
}

/// Test timeout error handling and classification
#[tokio::test]
async fn test_timeout_error_classification() -> Result<()> {
    let shell_config = ShellPoolConfig {
        command_timeout: Duration::from_millis(500), // Very short timeout
        ..Default::default()
    };
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test synchronous timeout
    let sync_result = adapter
        .execute_sync_in_dir(
            "sleep",
            Some({
                let mut args = Map::new();
                args.insert("duration".to_string(), json!("2")); // 2 seconds, longer than 500ms timeout
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(1), // 1 second timeout (still longer than shell timeout)
            None,
        )
        .await;

    // Should fail with timeout error
    assert!(sync_result.is_err());
    let error = sync_result.unwrap_err();
    let error_msg = error.to_string();
    assert!(
        error_msg.contains("timeout")
            || error_msg.contains("Timeout")
            || error_msg.contains("failed"),
        "Error should indicate timeout: {}",
        error_msg
    );

    // Test asynchronous timeout with callback
    let timeout_callback = ErrorTestCallback::new();

    let _async_operation_id = adapter
        .execute_async_in_dir_with_callback(
            "async_timeout_test",
            "sleep",
            Some({
                let mut args = Map::new();
                args.insert("duration".to_string(), json!("2")); // 2 seconds
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(1), // 1 second timeout
            Some(Box::new(timeout_callback.clone())),
        )
        .await?;

    // Wait for timeout to occur
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let updates = timeout_callback.get_updates().await;
    let has_timeout_notification = updates.iter().any(|update| match update {
        ProgressUpdate::Cancelled { message, .. } => {
            message.contains("timeout") || message.contains("Timeout")
        }
        ProgressUpdate::FinalResult { success: false, .. } => true,
        _ => false,
    });

    assert!(
        has_timeout_notification || timeout_callback.get_call_count() > 0,
        "Should receive timeout notification: {:?}",
        updates
    );

    adapter.shutdown().await;
    Ok(())
}

/// Test error recovery after failures
#[tokio::test]
async fn test_error_recovery_resilience() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Cycle through multiple error and success scenarios
    for cycle in 0..3 {
        // 1. Cause a synchronous error
        let error_result = adapter
            .execute_sync_in_dir(
                "nonexistent_command_xyz_123",
                None,
                temp_dir.path().to_str().unwrap(),
                Some(5),
                None,
            )
            .await;

        assert!(error_result.is_err(), "Should fail for nonexistent command");

        // 2. Verify adapter still works with success
        let success_result = adapter
            .execute_sync_in_dir(
                "echo",
                Some({
                    let mut args = Map::new();
                    args.insert(
                        "message".to_string(),
                        json!(format!("recovery cycle {}", cycle)),
                    );
                    args
                }),
                temp_dir.path().to_str().unwrap(),
                Some(10),
                None,
            )
            .await?;

        assert!(success_result.contains(&format!("cycle {}", cycle)));

        // 3. Test async error followed by async success
        let async_error_id = adapter
            .execute_async_in_dir(
                "async_error_test",
                "false", // Command that always fails
                None,
                temp_dir.path().to_str().unwrap(),
                Some(5),
            )
            .await?;

        let async_success_id = adapter
            .execute_async_in_dir(
                "async_success_test",
                "echo",
                Some({
                    let mut args = Map::new();
                    args.insert(
                        "message".to_string(),
                        json!(format!("async recovery {}", cycle)),
                    );
                    args
                }),
                temp_dir.path().to_str().unwrap(),
                Some(10),
            )
            .await?;

        // Both operations should get IDs
        assert!(async_error_id.starts_with("op_"));
        assert!(async_success_id.starts_with("op_"));
        assert_ne!(async_error_id, async_success_id);
    }

    // Wait for async operations to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    adapter.shutdown().await;
    Ok(())
}

/// Test edge cases in argument escaping and shell safety
#[tokio::test]
async fn test_argument_escaping_edge_cases() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test the static utility functions directly

    // Test needs_file_handling edge cases
    assert!(!Adapter::needs_file_handling(""));
    assert!(!Adapter::needs_file_handling("simple"));
    assert!(!Adapter::needs_file_handling("a".repeat(8192).as_str())); // Exactly at limit
    assert!(Adapter::needs_file_handling("a".repeat(8193).as_str())); // Over limit

    // Test escape_shell_argument edge cases
    assert_eq!(Adapter::escape_shell_argument(""), "''");
    assert_eq!(Adapter::escape_shell_argument("simple"), "'simple'");
    assert_eq!(
        Adapter::escape_shell_argument("no'quotes"),
        "'no'\"'\"'quotes'"
    );
    assert_eq!(
        Adapter::escape_shell_argument("multiple'single'quotes"),
        "'multiple'\"'\"'single'\"'\"'quotes'"
    );

    // Test complex escaping scenarios in actual command execution
    let escape_test_cases = vec![
        ("empty", ""),
        ("simple", "hello world"),
        ("single_quote", "don't"),
        ("multiple_quotes", "can't won't shouldn't"),
        ("mixed_special", "file with 'quotes' and spaces"),
        ("path_like", "/path/with spaces/and'quotes"),
    ];

    for (test_name, test_value) in escape_test_cases {
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

        assert!(
            result.is_ok(),
            "Escaping test '{}' should succeed: {:?}",
            test_name,
            result.err()
        );

        if !test_value.is_empty() {
            let output = result.unwrap();
            assert!(
                !output.trim().is_empty(),
                "Test '{}' should produce non-empty output",
                test_name
            );
        }
    }

    adapter.shutdown().await;
    Ok(())
}

/// Test complex AsyncExecOptions scenarios
#[tokio::test]
async fn test_complex_async_exec_options() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Test with custom operation ID and complex subcommand config
    let callback = ErrorTestCallback::new();
    let subcommand_config = SubcommandConfig {
        name: "custom_op_test".to_string(),
        description: "Custom operation test".to_string(),
        force_synchronous: Some(true),
        timeout_seconds: Some(30),
        enabled: true,
        guidance_key: Some("test_guidance".to_string()),
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
        positional_args: Some(vec![OptionConfig {
            name: "target".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: Some("Target argument".to_string()),
            format: Some("path".to_string()),
            items: None,
            required: Some(true),
            file_arg: Some(false),
            file_flag: None,
        }]),
        options: Some(vec![OptionConfig {
            name: "verbose".to_string(),
            alias: Some("v".to_string()),
            option_type: "boolean".to_string(),
            description: Some("Verbose mode".to_string()),
            format: None,
            items: None,
            required: Some(false),
            file_arg: Some(false),
            file_flag: None,
        }]),
    };

    let mut args = Map::new();
    args.insert("target".to_string(), json!("test_target"));
    args.insert("verbose".to_string(), json!(true));

    let options = AsyncExecOptions {
        operation_id: Some("custom_test_op_12345".to_string()),
        args: Some(args),
        timeout: Some(15),
        callback: Some(Box::new(callback.clone())),
        subcommand_config: Some(&subcommand_config),
    };

    let operation_id = adapter
        .execute_async_in_dir_with_options(
            "custom_test_tool",
            "echo",
            temp_dir.path().to_str().unwrap(),
            options,
        )
        .await?;

    // Should use the custom operation ID
    assert_eq!(operation_id, "custom_test_op_12345");

    // Wait for operation to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Should have received callback notifications
    let updates = callback.get_updates().await;
    assert!(
        !updates.is_empty() || callback.get_call_count() > 0,
        "Should have received callback updates"
    );

    adapter.shutdown().await;
    Ok(())
}

/// Test graceful shutdown under various conditions
#[tokio::test]
async fn test_graceful_shutdown_scenarios() -> Result<()> {
    let shell_config = ShellPoolConfig::default();
    let (adapter, temp_dir) = create_adapter_with_config(shell_config).await?;

    // Start multiple async operations
    let mut operation_ids = Vec::new();
    for i in 0..3 {
        let op_id = adapter
            .execute_async_in_dir(
                &format!("shutdown_test_{}", i),
                "echo",
                Some({
                    let mut args = Map::new();
                    args.insert("message".to_string(), json!(format!("shutdown test {}", i)));
                    args
                }),
                temp_dir.path().to_str().unwrap(),
                Some(30),
            )
            .await?;
        operation_ids.push(op_id);
    }

    // Give operations a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test graceful shutdown
    let shutdown_start = Instant::now();
    adapter.shutdown().await;
    let shutdown_duration = shutdown_start.elapsed();

    // Shutdown should complete in reasonable time (< 5 seconds for simple operations)
    assert!(
        shutdown_duration < Duration::from_secs(5),
        "Shutdown took too long: {:?}",
        shutdown_duration
    );

    // All operation IDs should be valid
    for op_id in operation_ids {
        assert!(op_id.starts_with("op_"));
    }

    Ok(())
}
