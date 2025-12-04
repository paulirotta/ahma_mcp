//! Realistic notification scenario tests with full adapter/service setup
//!
//! Consolidated from: realistic_endless_notification_test.rs

use ahma_core::adapter::Adapter;
use ahma_core::config::load_tool_configs;
use ahma_core::mcp_service::AhmaMcpService;
use ahma_core::operation_monitor::{MonitorConfig, OperationMonitor};
use ahma_core::shell_pool::{ShellPoolConfig, ShellPoolManager};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// This test reproduces a scenario similar to the user's logs, but adapted
/// for the `completion_history` architecture.
/// 1. Run an async operation (like cargo version).
/// 2. Repeatedly check the `completion_history`.
/// 3. Ensure the operation appears once and is not duplicated on subsequent checks.
#[tokio::test]
async fn test_realistic_notification_scenario_with_history() {
    // Setup
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
    shell_pool_manager.clone().start_background_tasks();
    let adapter =
        Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager.clone()).unwrap());
    let configs = Arc::new(
        load_tool_configs(&std::path::PathBuf::from(".ahma/tools"))
            .await
            .unwrap(),
    );
    let _service = AhmaMcpService::new(
        adapter.clone(),
        operation_monitor.clone(),
        configs,
        Arc::new(None),
        false,
    )
    .await
    .unwrap();

    // Execute async operation
    let operation_id = adapter
        .execute_async_in_dir(
            "cargo",
            "version",
            Some(serde_json::Map::from_iter(vec![(
                "_subcommand".to_string(),
                serde_json::Value::String("version".to_string()),
            )])),
            ".",
            Some(30),
        )
        .await
        .expect("Failed to execute async operation");

    // Wait for the operation to appear in the completion history
    let completed_op = operation_monitor.wait_for_operation(&operation_id).await;
    assert!(
        completed_op.is_some(),
        "Operation never appeared in completion history"
    );

    // Now, simulate a notification loop checking the history
    let mut seen_operations = HashSet::new();
    let mut notifications_sent = 0;

    // Check the history multiple times
    for _i in 1..=5 {
        let completed_ops = operation_monitor.get_completed_operations().await;

        for op in completed_ops {
            // In a real notification system, we'd check if we've already notified for this op.
            if !seen_operations.contains(&op.id) {
                seen_operations.insert(op.id.clone());
                notifications_sent += 1;
            }
        }
    }

    // We should only have sent one notification for the single operation
    assert_eq!(
        notifications_sent, 1,
        "Should have sent exactly one notification."
    );
}

/// Test if multiple operations are handled correctly by the completion history.
#[tokio::test]
async fn test_multiple_operations_notification_behavior() {
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
    shell_pool_manager.clone().start_background_tasks();
    let adapter =
        Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager).unwrap());

    // Start multiple operations
    let op_ids = vec![
        adapter
            .execute_async_in_dir("cargo", "version", None, ".", Some(30))
            .await
            .expect("Failed to execute first async operation"),
        adapter
            .execute_async_in_dir("cargo", "--version", None, ".", Some(30))
            .await
            .expect("Failed to execute second async operation"),
    ];

    // Wait for both to complete
    for op_id in &op_ids {
        let completed_op = operation_monitor.wait_for_operation(op_id).await;
        assert!(
            completed_op.is_some(),
            "Operation {} did not complete in time",
            op_id
        );
    }
    let final_completed = operation_monitor.get_completed_operations().await;
    assert_eq!(
        final_completed.len(),
        op_ids.len(),
        "Not all operations completed in time."
    );

    // Simulate notification loop
    let mut all_seen_operations = HashSet::new();
    let mut total_notifications = 0;

    for _iteration in 1..=4 {
        let completed_ops = operation_monitor.get_completed_operations().await;

        for op in completed_ops {
            if all_seen_operations.insert(op.id.clone()) {
                total_notifications += 1;
            }
        }
    }

    assert_eq!(
        total_notifications,
        op_ids.len(),
        "Incorrect number of notifications sent."
    );
}
