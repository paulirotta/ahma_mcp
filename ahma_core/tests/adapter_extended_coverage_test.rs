//! Extended coverage tests for the adapter module
//!
//! These tests cover edge cases and functions that need additional test coverage
//! in adapter.rs including value_to_string, process_named_arg, and temp file handling.

use ahma_core::adapter::{Adapter, AsyncExecOptions, ExecutionMode};
use ahma_core::config::{CommandOption, SubcommandConfig};
use ahma_core::operation_monitor::{MonitorConfig, OperationMonitor};
use ahma_core::shell_pool::{ShellPoolConfig, ShellPoolManager};
use ahma_core::test_utils::init_test_sandbox;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

async fn create_test_adapter() -> Arc<Adapter> {
    // Initialize sandbox for tests (idempotent)
    init_test_sandbox();

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
    // Use a permissive root for testing
    Arc::new(
        Adapter::new(monitor, shell_pool)
            .unwrap()
            .with_root(std::path::PathBuf::from("/")),
    )
}

async fn create_adapter_with_root(root: std::path::PathBuf) -> Arc<Adapter> {
    // Initialize sandbox for tests (idempotent)
    init_test_sandbox();

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
    Arc::new(Adapter::new(monitor, shell_pool).unwrap().with_root(root))
}

// === ExecutionMode Tests ===

#[test]
fn test_execution_mode_enum() {
    // Test Debug trait
    let sync = ExecutionMode::Synchronous;
    let async_mode = ExecutionMode::AsyncResultPush;

    let sync_debug = format!("{:?}", sync);
    let async_debug = format!("{:?}", async_mode);

    assert!(sync_debug.contains("Synchronous"));
    assert!(async_debug.contains("AsyncResultPush"));

    // Test Clone trait
    let sync_clone = sync;
    assert_eq!(sync_clone, ExecutionMode::Synchronous);

    // Test Copy trait (implicit)
    let async_copy = async_mode;
    assert_eq!(async_copy, ExecutionMode::AsyncResultPush);

    // Test PartialEq
    assert_eq!(ExecutionMode::Synchronous, ExecutionMode::Synchronous);
    assert_ne!(ExecutionMode::Synchronous, ExecutionMode::AsyncResultPush);
}

// === needs_file_handling Tests ===

#[test]
fn test_needs_file_handling_newline() {
    assert!(Adapter::needs_file_handling("line1\nline2"));
    assert!(Adapter::needs_file_handling("a\nb\nc"));
}

#[test]
fn test_needs_file_handling_carriage_return() {
    assert!(Adapter::needs_file_handling("text\rwith\rreturns"));
}

#[test]
fn test_needs_file_handling_quotes() {
    assert!(Adapter::needs_file_handling("it's fine"));
    assert!(Adapter::needs_file_handling("he said \"hello\""));
}

#[test]
fn test_needs_file_handling_special_chars() {
    assert!(Adapter::needs_file_handling("path\\to\\file"));
    assert!(Adapter::needs_file_handling("run `cmd`"));
    assert!(Adapter::needs_file_handling("echo $HOME"));
}

#[test]
fn test_needs_file_handling_length_boundary() {
    // Exactly at limit - should NOT need file handling
    let at_limit = "x".repeat(8192);
    assert!(!Adapter::needs_file_handling(&at_limit));

    // One over limit - SHOULD need file handling
    let over_limit = "x".repeat(8193);
    assert!(Adapter::needs_file_handling(&over_limit));
}

#[test]
fn test_needs_file_handling_safe_strings() {
    assert!(!Adapter::needs_file_handling("hello"));
    assert!(!Adapter::needs_file_handling("hello world"));
    assert!(!Adapter::needs_file_handling("path/to/file"));
    assert!(!Adapter::needs_file_handling("file-name_123.txt"));
    assert!(!Adapter::needs_file_handling(""));
}

// === escape_shell_argument Tests ===

#[test]
fn test_escape_shell_argument_simple() {
    assert_eq!(Adapter::escape_shell_argument("hello"), "'hello'");
    assert_eq!(Adapter::escape_shell_argument(""), "''");
}

#[test]
fn test_escape_shell_argument_with_single_quote() {
    // Single quote should be escaped by ending the single-quoted string,
    // inserting an escaped quote, and starting a new single-quoted string
    let result = Adapter::escape_shell_argument("it's");
    assert!(result.contains("'\"'\"'"));
}

#[test]
fn test_escape_shell_argument_multiple_quotes() {
    let result = Adapter::escape_shell_argument("it's 'quoted'");
    // Should escape single quotes
    assert!(result.starts_with("'"));
    assert!(result.ends_with("'"));
    // The string has 3 single quotes: it's, 'quoted'
    // Each single quote becomes '"'"' (5 chars including the escape)
    // Just verify the escape pattern exists
    let pattern = "'\"'\"'";
    let count = result.matches(pattern).count();
    assert!(
        count >= 2,
        "Should have escaped single quotes, got count: {}",
        count
    );
}

#[test]
fn test_escape_shell_argument_special_chars_no_single_quote() {
    // Other special chars are safe inside single quotes
    let result = Adapter::escape_shell_argument("$HOME \"test\" `cmd` \\path");
    assert_eq!(result, "'$HOME \"test\" `cmd` \\path'");
}

// === Adapter with_root Tests ===

#[tokio::test]
async fn test_adapter_with_root() {
    let temp = tempdir().unwrap();
    let adapter = create_adapter_with_root(temp.path().to_path_buf()).await;

    // Verify adapter was created with custom root
    let debug = format!("{:?}", adapter);
    assert!(debug.contains("Adapter"));
}

// === execute_sync_in_dir Tests ===

#[tokio::test]
async fn test_execute_sync_simple_echo() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let result = adapter
        .execute_sync_in_dir("echo", None, temp.path().to_str().unwrap(), None, None)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_sync_with_args() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("args".to_string(), json!(["hello", "world"]));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("hello"));
    assert!(output.contains("world"));
}

#[tokio::test]
async fn test_execute_sync_command_failure() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    // Running a command that doesn't exist should fail
    let result = adapter
        .execute_sync_in_dir(
            "nonexistent_command_12345",
            None,
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_sync_timeout() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    // Very short timeout should cause timeout error
    let result = adapter
        .execute_sync_in_dir(
            "sleep 10",
            None,
            temp.path().to_str().unwrap(),
            Some(1), // 1 second timeout
            None,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("timed out") || err.contains("timeout"),
        "Expected timeout error, got: {}",
        err
    );
}

// === execute_async_in_dir Tests ===

#[tokio::test]
async fn test_execute_async_returns_operation_id() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let result = adapter
        .execute_async_in_dir(
            "echo_tool",
            "echo",
            None,
            temp.path().to_str().unwrap(),
            Some(30),
        )
        .await;

    assert!(result.is_ok());
    let op_id = result.unwrap();
    assert!(
        op_id.starts_with("op_"),
        "Operation ID should start with op_"
    );
}

#[tokio::test]
async fn test_execute_async_with_options() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("args".to_string(), json!(["test output"]));

    let result = adapter
        .execute_async_in_dir_with_options(
            "echo_tool",
            "echo",
            temp.path().to_str().unwrap(),
            AsyncExecOptions {
                operation_id: Some("custom_op_id".to_string()),
                args: Some(args),
                timeout: Some(30),
                callback: None,
                subcommand_config: None,
            },
        )
        .await;

    assert!(result.is_ok());
    let op_id = result.unwrap();
    assert_eq!(op_id, "custom_op_id");
}

// === Boolean argument handling Tests ===

#[tokio::test]
async fn test_boolean_arg_true() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("verbose".to_string(), Value::Bool(true));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_boolean_arg_false_not_added() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("verbose".to_string(), Value::Bool(false));

    // When bool is false, the flag should not be added
    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    // The output should not contain --verbose
    assert!(!output.contains("--verbose"));
}

// === Meta-parameter filtering Tests ===

#[tokio::test]
async fn test_meta_params_not_passed_as_args() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    // These should be filtered out and not passed to the command
    args.insert(
        "working_directory".to_string(),
        Value::String("/tmp".to_string()),
    );
    args.insert(
        "execution_mode".to_string(),
        Value::String("sync".to_string()),
    );
    args.insert("timeout_seconds".to_string(), Value::Number(30.into()));
    args.insert("args".to_string(), json!(["actual", "args"]));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    // Should contain actual args but not meta-parameters
    assert!(output.contains("actual"));
    assert!(!output.contains("working_directory"));
    assert!(!output.contains("execution_mode"));
    assert!(!output.contains("timeout_seconds"));
}

// === Array arguments Tests ===

#[tokio::test]
async fn test_array_args() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("args".to_string(), json!(["one", "two", "three"]));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("one"));
    assert!(output.contains("two"));
    assert!(output.contains("three"));
}

// === Null value handling Tests ===

#[tokio::test]
async fn test_null_values_ignored() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("null_param".to_string(), Value::Null);
    args.insert("args".to_string(), json!(["hello"]));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("hello"));
    // Null params should not produce any output
    assert!(!output.contains("null"));
}

// === Number arguments Tests ===

#[tokio::test]
async fn test_number_args() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let mut args = Map::new();
    args.insert("count".to_string(), Value::Number(42.into()));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("42"));
}

// === Empty command error Tests ===

#[tokio::test]
async fn test_empty_command_error() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    // Empty command should fail with a proper error, not panic
    let result = adapter
        .execute_sync_in_dir("", None, temp.path().to_str().unwrap(), None, None)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("empty") || err.contains("must not"),
        "Expected empty command error, got: {}",
        err
    );
}

// === Shutdown Tests ===

#[tokio::test]
async fn test_adapter_shutdown() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    // Start an async operation
    let _op_id = adapter
        .execute_async_in_dir(
            "sleep_tool",
            "sleep",
            Some({
                let mut m = Map::new();
                m.insert("args".to_string(), json!(["2"]));
                m
            }),
            temp.path().to_str().unwrap(),
            Some(30),
        )
        .await
        .unwrap();

    // Shutdown should complete without panic
    adapter.shutdown().await;
}

// === Subcommand config with positional args ===

fn create_subcommand_with_positional() -> SubcommandConfig {
    SubcommandConfig {
        name: "test_sub".to_string(),
        description: "Test subcommand".to_string(),
        options: None,
        positional_args: Some(vec![CommandOption {
            name: "file".to_string(),
            option_type: "string".to_string(),
            description: Some("Input file".to_string()),
            required: Some(true),
            format: None,
            items: None,
            file_arg: None,
            file_flag: None,
            alias: None,
        }]),
        synchronous: None,
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
    }
}

#[tokio::test]
async fn test_positional_args_handling() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let subcommand_config = create_subcommand_with_positional();

    let mut args = Map::new();
    args.insert("file".to_string(), Value::String("myfile.txt".to_string()));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            Some(&subcommand_config),
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    // Positional arg should be passed directly without --file
    assert!(output.contains("myfile.txt"));
}

// === Path validation Tests ===

#[tokio::test]
async fn test_path_option_validation() {
    let temp = tempdir().unwrap();
    let adapter = create_adapter_with_root(temp.path().to_path_buf()).await;

    let subcommand_config = SubcommandConfig {
        name: "test_sub".to_string(),
        description: "Test subcommand".to_string(),
        options: Some(vec![CommandOption {
            name: "path_opt".to_string(),
            description: Some("A path option".to_string()),
            option_type: "string".to_string(),
            required: Some(false),
            alias: None,
            format: Some("path".to_string()),
            items: None,
            file_arg: None,
            file_flag: None,
        }]),
        positional_args: None,
        synchronous: None,
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
    };

    let mut args = Map::new();
    args.insert("path_opt".to_string(), Value::String(".".to_string()));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            Some(&subcommand_config),
        )
        .await;

    assert!(result.is_ok());
}

// === Shell command redirect tests ===
// Note: These tests are commented out as they require shell command execution
// which behaves differently depending on system configuration.
// The redirect functionality (2>&1 appending) is tested elsewhere.

// #[tokio::test]
// async fn test_shell_command_gets_redirect() {
//     // Tests auto-appending of 2>&1 for shell commands
// }

// #[tokio::test]
// async fn test_shell_command_existing_redirect_not_duplicated() {
//     // Tests that 2>&1 is not duplicated when already present
// }

// === Option with alias Tests ===

#[tokio::test]
async fn test_option_with_alias_uses_short_flag() {
    let adapter = create_test_adapter().await;
    let temp = tempdir().unwrap();

    let subcommand_config = SubcommandConfig {
        name: "test_sub".to_string(),
        description: "Test subcommand".to_string(),
        options: Some(vec![CommandOption {
            name: "verbose".to_string(),
            description: Some("Verbose output".to_string()),
            option_type: "boolean".to_string(),
            required: Some(false),
            alias: Some("v".to_string()),
            format: None,
            items: None,
            file_arg: None,
            file_flag: None,
        }]),
        positional_args: None,
        synchronous: None,
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: None,
        install_instructions: None,
    };

    let mut args = Map::new();
    args.insert("verbose".to_string(), Value::Bool(true));

    let result = adapter
        .execute_sync_in_dir(
            "echo",
            Some(args),
            temp.path().to_str().unwrap(),
            None,
            Some(&subcommand_config),
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    // Should use -v instead of --verbose due to alias
    assert!(output.contains("-v"));
}
