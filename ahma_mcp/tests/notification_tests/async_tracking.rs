//! Tests for async operation tracking and result clarity
//!
//! Consolidated from: async_callback_notification_test.rs, async_notification_debug.rs

use ahma_mcp::{
    adapter::Adapter,
    config::load_tool_configs,
    mcp_service::AhmaMcpService,
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
    test_utils::init_test_sandbox,
};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;

#[tokio::test]
async fn test_async_operations_complete_and_are_tracked() -> anyhow::Result<()> {
    use ahma_mcp::test_utils::concurrency::*;
    init_test_sandbox();

    // Set up the test environment using TempDir
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
        Adapter::new(operation_monitor.clone(), shell_pool)
            .expect("Failed to create adapter")
            .with_root(temp_dir.path().to_path_buf()),
    );

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

    // Wait for the operation to complete using CI-resilient helper
    let result = with_ci_timeout(
        "operation completion",
        CI_DEFAULT_TIMEOUT,
        operation_monitor.wait_for_operation(&operation_id),
    )
    .await?;

    assert!(
        result.is_some(),
        "Operation should complete within the timeout period"
    );

    let completed_op = result.unwrap();

    // Verify the operation was tracked and has results
    assert_eq!(completed_op.id, operation_id, "Operation ID should match");
    assert!(
        completed_op.result.is_some(),
        "Completed operation should have results"
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

    assert!(
        exit_code.is_some(),
        "Operation result should include exit code"
    );

    // Verify the operation completed successfully
    assert_eq!(
        exit_code,
        Some(0),
        "Echo command should complete successfully"
    );

    assert!(
        stdout.contains("Hello from callback test"),
        "stdout should contain the expected text"
    );

    // Test that the operation appears in completed operations
    let completed_ops = operation_monitor.get_completed_operations().await;
    assert!(
        completed_ops.iter().any(|op| op.id == operation_id),
        "Completed operation should appear in completed operations list"
    );
    Ok(())
}

#[tokio::test]
async fn test_operation_monitoring_provides_clear_results() -> anyhow::Result<()> {
    use ahma_mcp::test_utils::concurrency::*;
    init_test_sandbox();

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
        Adapter::new(operation_monitor.clone(), shell_pool)
            .expect("Failed to create adapter")
            .with_root(temp_dir.path().to_path_buf()),
    );

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

    let success_result = with_ci_timeout(
        "success operation",
        CI_DEFAULT_TIMEOUT,
        operation_monitor.wait_for_operation(&success_id),
    )
    .await?
    .expect("Operation should complete");

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

    // Test failing command
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

    let failure_result = with_ci_timeout(
        "failure operation",
        CI_DEFAULT_TIMEOUT,
        operation_monitor.wait_for_operation(&failure_id),
    )
    .await?
    .expect("Operation should complete");

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

    // Verify that both operations are tracked
    let all_completed = operation_monitor.get_completed_operations().await;
    assert!(
        all_completed.len() >= 2,
        "Both operations should be tracked"
    );
    Ok(())
}

#[tokio::test]
async fn test_operation_completion_tracking() -> anyhow::Result<()> {
    use ahma_mcp::test_utils::concurrency::*;
    use ahma_mcp::test_utils::project::*;

    // Create a temporary project using specialized helper
    let temp = create_rust_project(TestProjectOptions {
        with_cargo: true,
        with_tool_configs: true,
        ..Default::default()
    })
    .await?;

    // Create the components
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config.clone()));

    let shell_pool_config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 20,
        shell_idle_timeout: Duration::from_secs(1800),
        pool_cleanup_interval: Duration::from_secs(300),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(60),
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
    let adapter = Arc::new(
        Adapter::new(operation_monitor.clone(), shell_pool_manager)
            .unwrap()
            .with_root(temp.path().to_path_buf()),
    );

    // Load configs from the temp PROJECT, not the repository
    let configs = Arc::new(load_tool_configs(&temp.path().join(".ahma")).await.unwrap());

    let _service = AhmaMcpService::new(
        adapter.clone(),
        operation_monitor.clone(),
        configs,
        Arc::new(None),
        false,
        false,
    )
    .await
    .unwrap();

    // Test using direct adapter call to see what happens with the fix
    let job_id = adapter
        .execute_async_in_dir(
            "cargo",   // Use base command
            "version", // command
            Some(serde_json::Map::from_iter(vec![(
                "_subcommand".to_string(),
                serde_json::Value::String("version".to_string()),
            )])),
            temp.path().to_str().unwrap(),
            Some(10),
        )
        .await
        .expect("Failed to execute async operation");

    // Wait for completion with CI-resilient helper
    let completed_op = with_ci_timeout(
        "cargo version completion",
        CI_DEFAULT_TIMEOUT,
        operation_monitor.wait_for_operation(&job_id),
    )
    .await?;

    assert!(completed_op.is_some(), "Operation did not complete in time");

    let completed_ops = operation_monitor.get_completed_operations().await;
    assert!(
        !completed_ops.is_empty(),
        "Should have completed operations"
    );
    Ok(())
}
