/// Test to verify that async callback notifications are delivered properly to AI agents
/// This addresses the issue where nextest completed but no callback notification was received
use ahma_mcp::{
    adapter::Adapter,
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;

#[tokio::test]
async fn test_async_operations_complete_and_are_tracked() {
    println!("🧪 Testing that async operations complete and are properly tracked...");

    // Set up the test environment
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_config = ShellPoolConfig {
        enabled: true,
        command_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    let adapter = Arc::new(
        Adapter::new(operation_monitor.clone(), shell_pool).expect("Failed to create adapter"),
    );

    println!("🚀 Starting a fast operation that should complete quickly...");

    // Start a simple, fast operation that should complete quickly
    let operation_id = adapter
        .execute_async_in_dir(
            "test_callback",
            "echo",
            Some({
                let mut args = serde_json::Map::new();
                args.insert(
                    "text".to_string(),
                    serde_json::Value::String("Hello from callback test".to_string()),
                );
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(10), // 10 second timeout
        )
        .await
        .expect("Failed to start operation");

    println!("✅ Operation started with ID: {}", operation_id);

    // Wait for the operation to complete
    println!("⏳ Waiting for operation to complete...");
    let result = operation_monitor.wait_for_operation(&operation_id).await;

    assert!(
        result.is_some(),
        "❌ Operation should complete within the timeout period"
    );

    let completed_op = result.unwrap();
    println!("� Operation completed: {:?}", completed_op);

    // Verify the operation was tracked and has results
    assert_eq!(completed_op.id, operation_id, "Operation ID should match");
    assert!(
        completed_op.result.is_some(),
        "❌ Completed operation should have results"
    );

    let op_result = completed_op.result.as_ref().unwrap();

    // Access JSON fields properly
    let exit_code = op_result
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let stdout = op_result
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stderr = op_result
        .get("stderr")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert!(
        exit_code.is_some(),
        "❌ Operation result should include exit code"
    );

    println!("📤 Exit Code: {:?}", exit_code);
    println!("📤 stdout: '{}'", stdout);
    println!("📤 stderr: '{}'", stderr);

    // Verify the operation completed successfully
    assert_eq!(
        exit_code,
        Some(0),
        "❌ Echo command should complete successfully"
    );

    assert!(
        stdout.contains("Hello from callback test"),
        "❌ stdout should contain the expected text"
    );

    println!("✅ Async operation tracking and completion works correctly!");

    // Test that the operation appears in completed operations
    let completed_ops = operation_monitor.get_completed_operations().await;
    assert!(
        completed_ops.iter().any(|op| op.id == operation_id),
        "❌ Completed operation should appear in completed operations list"
    );

    println!("✅ Operation appears correctly in completed operations list!");
}

#[tokio::test]
async fn test_operation_monitoring_provides_clear_results() {
    println!(
        "🧪 Testing that operation results provide clear information for AI decision-making..."
    );

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_config = ShellPoolConfig {
        enabled: true,
        command_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    let adapter = Arc::new(
        Adapter::new(operation_monitor.clone(), shell_pool).expect("Failed to create adapter"),
    );

    // Test with a command that produces both success and failure cases
    println!("🧪 Testing successful command...");

    // Test successful command
    let success_id = adapter
        .execute_async_in_dir(
            "test_success",
            "echo",
            Some({
                let mut args = serde_json::Map::new();
                args.insert(
                    "text".to_string(),
                    serde_json::Value::String("Success test".to_string()),
                );
                args
            }),
            temp_dir.path().to_str().unwrap(),
            Some(10),
        )
        .await
        .expect("Failed to start success operation");

    let success_result = operation_monitor
        .wait_for_operation(&success_id)
        .await
        .unwrap();
    let success_result_data = success_result.result.as_ref().unwrap();

    let success_exit_code = success_result_data
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let success_stdout = success_result_data
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_eq!(
        success_exit_code,
        Some(0),
        "Success case should have exit code 0"
    );
    assert!(
        success_stdout.contains("Success test"),
        "Success case should have expected output"
    );

    println!("✅ Successful command provides clear success indicators");

    // Test failing command
    println!("🧪 Testing failing command...");

    let failure_id = adapter
        .execute_async_in_dir(
            "test_failure",
            "false", // This command always returns exit code 1
            None,
            temp_dir.path().to_str().unwrap(),
            Some(10),
        )
        .await
        .expect("Failed to start failure operation");

    let failure_result = operation_monitor
        .wait_for_operation(&failure_id)
        .await
        .unwrap();
    let failure_result_data = failure_result.result.as_ref().unwrap();

    let failure_exit_code = failure_result_data
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

    assert_ne!(
        failure_exit_code,
        Some(0),
        "Failure case should have non-zero exit code"
    );

    println!("✅ Failed command provides clear failure indicators");
    println!("📋 Exit codes are clearly distinguishable for AI decision-making");

    // Verify that both operations are tracked
    let all_completed = operation_monitor.get_completed_operations().await;
    assert!(
        all_completed.len() >= 2,
        "Both operations should be tracked"
    );

    println!("✅ Operation monitoring provides clear, actionable results for AI!");
}
