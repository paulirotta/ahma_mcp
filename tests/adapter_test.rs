use ahma_mcp::adapter::{Adapter, AsyncExecOptions, ExecutionMode};
use ahma_mcp::config::{OptionConfig, SubcommandConfig};
use ahma_mcp::operation_monitor::{MonitorConfig, OperationMonitor};
use ahma_mcp::shell_pool::{ShellPoolConfig, ShellPoolManager};
use serde_json::{Map, Value};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

async fn create_test_adapter() -> Adapter {
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
    Adapter::new(monitor, shell_pool).unwrap()
}

#[tokio::test]
async fn test_adapter_creation() {
    let _adapter = create_test_adapter().await;
    // Basic creation test - if it doesn't panic, it works
    // Test passed if we reach this point
}

#[tokio::test]
async fn test_needs_file_handling() {
    // Test cases that should require file handling
    assert!(Adapter::needs_file_handling("line1\nline2"));
    assert!(Adapter::needs_file_handling(
        "content with\rcarriage return"
    ));
    assert!(Adapter::needs_file_handling("string with 'single quotes'"));
    assert!(Adapter::needs_file_handling(
        "string with \"double quotes\""
    ));
    assert!(Adapter::needs_file_handling("string with \\backslash"));
    assert!(Adapter::needs_file_handling("string with `backticks`"));
    assert!(Adapter::needs_file_handling("string with $variables"));

    // Test very long string
    let long_string = "a".repeat(9000);
    assert!(Adapter::needs_file_handling(&long_string));

    // Test cases that should NOT require file handling
    assert!(!Adapter::needs_file_handling("simple string"));
    assert!(!Adapter::needs_file_handling("string with spaces"));
    assert!(!Adapter::needs_file_handling("string-with-dashes"));
    assert!(!Adapter::needs_file_handling("string_with_underscores"));
    assert!(!Adapter::needs_file_handling("123456789"));
    assert!(!Adapter::needs_file_handling(""));

    // Edge case: exactly at the length limit
    let exactly_limit = "a".repeat(8192);
    assert!(!Adapter::needs_file_handling(&exactly_limit));

    let over_limit = "a".repeat(8193);
    assert!(Adapter::needs_file_handling(&over_limit));
}

#[tokio::test]
async fn test_escape_shell_argument() {
    // Basic string without single quotes
    assert_eq!(
        Adapter::escape_shell_argument("hello world"),
        "'hello world'"
    );

    // String with single quotes - should be escaped
    assert_eq!(
        Adapter::escape_shell_argument("don't do it"),
        "'don'\"'\"'t do it'"
    );

    // Multiple single quotes
    assert_eq!(
        Adapter::escape_shell_argument("can't won't shouldn't"),
        "'can'\"'\"'t won'\"'\"'t shouldn'\"'\"'t'"
    );

    // Empty string
    assert_eq!(Adapter::escape_shell_argument(""), "''");

    // String with other special characters (but no single quotes)
    assert_eq!(
        Adapter::escape_shell_argument("hello \"world\" $var `cmd` \\path"),
        "'hello \"world\" $var `cmd` \\path'"
    );
}

#[tokio::test]
async fn test_execute_sync_basic() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Test simple echo command
    let mut args = Map::new();
    args.insert("text".to_string(), Value::String("hello world".to_string()));

    let result = adapter
        .execute_sync_in_dir("echo", Some(args), working_dir, Some(5), None)
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("hello") || output.contains("world") || output.contains("--text"));
}

#[tokio::test]
async fn test_execute_sync_with_subcommand_config() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Create a subcommand config for echo
    let subcommand_config = SubcommandConfig {
        name: "echo".to_string(),
        description: "Echo command".to_string(),
        options: Some(vec![OptionConfig {
            name: "n".to_string(),
            alias: None,
            option_type: "boolean".to_string(),
            description: "Do not print trailing newline".to_string(),
            format: None,
            required: Some(false),
            file_arg: Some(false),
            file_flag: None,
        }]),
        positional_args: Some(vec![OptionConfig {
            name: "message".to_string(),
            alias: None,
            option_type: "string".to_string(),
            description: "Message to echo".to_string(),
            format: None,
            required: Some(true),
            file_arg: Some(false),
            file_flag: None,
        }]),
        synchronous: Some(true),
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
    };

    let mut args = Map::new();
    args.insert(
        "message".to_string(),
        Value::String("test message".to_string()),
    );
    args.insert("n".to_string(), Value::Bool(true));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            working_dir,
            Some(5),
            Some(&subcommand_config),
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("test message") || output.contains("echo"));
}

#[tokio::test]
async fn test_execute_async_basic() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    let mut args = Map::new();
    args.insert("text".to_string(), Value::String("async test".to_string()));

    let operation_id = adapter
        .execute_async_in_dir("echo", "echo", Some(args), working_dir, Some(5))
        .await;

    assert!(operation_id.is_ok());
    let op_id = operation_id.unwrap();
    assert!(op_id.starts_with("op_"));

    // Give the async operation time to complete
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_execute_async_with_options() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    let mut args = Map::new();
    args.insert(
        "text".to_string(),
        Value::String("options test".to_string()),
    );

    let options = AsyncExecOptions {
        operation_id: Some("custom_op_123".to_string()),
        args: Some(args),
        timeout: Some(10),
        callback: None,
        subcommand_config: None,
    };

    let result = adapter
        .execute_async_in_dir_with_options("echo", "echo", working_dir, options)
        .await;

    assert!(result.is_ok());
    let op_id = result.unwrap();
    assert_eq!(op_id, "custom_op_123");
}

#[tokio::test]
async fn test_execute_async_with_callback() {
    use ahma_mcp::callback_system::{CallbackSender, ProgressUpdate};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // Create a test callback that captures updates
    #[derive(Clone)]
    struct TestCallback {
        updates: Arc<Mutex<Vec<ProgressUpdate>>>,
    }

    #[async_trait]
    impl CallbackSender for TestCallback {
        async fn send_progress(
            &self,
            update: ProgressUpdate,
        ) -> Result<(), ahma_mcp::callback_system::CallbackError> {
            self.updates.lock().await.push(update);
            Ok(())
        }

        async fn should_cancel(&self) -> bool {
            false
        }
    }

    let callback_updates = Arc::new(Mutex::new(Vec::new()));
    let callback = Box::new(TestCallback {
        updates: callback_updates.clone(),
    });

    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    let mut args = Map::new();
    args.insert(
        "text".to_string(),
        Value::String("callback test".to_string()),
    );

    let result = adapter
        .execute_async_in_dir_with_callback(
            "echo",
            "echo",
            Some(args),
            working_dir,
            Some(5),
            Some(callback),
        )
        .await;

    assert!(result.is_ok());

    // Give the async operation time to complete and send callback
    tokio::time::sleep(Duration::from_millis(200)).await;

    let updates = callback_updates.lock().await;
    assert!(!updates.is_empty());

    // Should have at least a final result update
    let has_final_result = updates
        .iter()
        .any(|update| matches!(update, ProgressUpdate::FinalResult { .. }));
    assert!(has_final_result);
}

#[tokio::test]
async fn test_execute_sync_command_failure() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Use a command that should fail
    let result = adapter
        .execute_sync_in_dir(
            "nonexistent_command_12345",
            None,
            working_dir,
            Some(5),
            None,
        )
        .await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("failed") || error.to_string().contains("not found"));
}

#[tokio::test]
async fn test_shutdown_graceful() {
    let adapter = create_test_adapter().await;

    // Test basic shutdown without any operations - this should be fast
    let shutdown_future = adapter.shutdown();
    let timeout_duration = Duration::from_secs(2);

    match tokio::time::timeout(timeout_duration, shutdown_future).await {
        Ok(_) => {
            // Shutdown completed successfully
            // Test passed
        }
        Err(_) => {
            panic!(
                "Shutdown timed out after {} seconds",
                timeout_duration.as_secs()
            );
        }
    }
}

#[tokio::test]
async fn test_execution_mode_enum() {
    assert_eq!(ExecutionMode::Synchronous, ExecutionMode::Synchronous);
    assert_eq!(
        ExecutionMode::AsyncResultPush,
        ExecutionMode::AsyncResultPush
    );
    assert_ne!(ExecutionMode::Synchronous, ExecutionMode::AsyncResultPush);
}

#[tokio::test]
async fn test_async_exec_options_creation() {
    let mut args = Map::new();
    args.insert("test".to_string(), Value::String("value".to_string()));

    let options = AsyncExecOptions {
        operation_id: Some("test_op".to_string()),
        args: Some(args.clone()),
        timeout: Some(30),
        callback: None,
        subcommand_config: None,
    };

    assert_eq!(options.operation_id, Some("test_op".to_string()));
    assert!(options.args.is_some());
    assert_eq!(options.timeout, Some(30));
    assert!(options.callback.is_none());
    assert!(options.subcommand_config.is_none());
}

#[tokio::test]
async fn test_adapter_with_empty_args() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Test command with no arguments
    let result = adapter
        .execute_sync_in_dir("pwd", None, working_dir, Some(5), None)
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(!output.is_empty());
}

#[tokio::test]
async fn test_adapter_with_boolean_args() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    let mut args = Map::new();
    args.insert("verbose".to_string(), Value::Bool(true));
    args.insert("quiet".to_string(), Value::Bool(false));

    let result = adapter
        .execute_sync_in_dir("echo", Some(args), working_dir, Some(5), None)
        .await;

    // Should not fail even if the arguments don't make sense for echo
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_adapter_with_array_args() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    let mut args = Map::new();
    args.insert(
        "items".to_string(),
        Value::Array(vec![
            Value::String("item1".to_string()),
            Value::String("item2".to_string()),
        ]),
    );

    let result = adapter
        .execute_sync_in_dir("echo", Some(args), working_dir, Some(5), None)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_long_running_command_timeout() {
    let adapter = create_test_adapter().await;
    let temp_dir = tempdir().unwrap();
    let working_dir = temp_dir.path().to_str().unwrap();

    // Use a sleep command that should timeout quickly
    let mut args = Map::new();
    args.insert("duration".to_string(), Value::String("10".to_string()));

    let result = adapter
        .execute_sync_in_dir("sleep", Some(args), working_dir, Some(1), None) // 1 second timeout
        .await;

    // Should either timeout or fail (depending on the system)
    assert!(result.is_err());
}
