//! Tests for operation completion history persistence and deduplication
//!
//! Consolidated from: endless_notification_fix_test.rs, continuous_update_test.rs

use ahma_mcp::operation_monitor::{
    MonitorConfig, Operation, OperationMonitor, OperationStatus,
};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_completed_operations_are_persistent_in_history() {
    // Verify that completed operations are moved to completion_history
    // and remain accessible for await operations while preventing duplicates

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    // Add a pending operation first
    let operation = Operation::new(
        "test-op-123".to_string(),
        "test".to_string(),
        "Test operation".to_string(),
        None,
    );
    operation_monitor.add_operation(operation).await;

    // Complete the operation (this should move it to completion_history)
    operation_monitor
        .update_status(
            "test-op-123",
            OperationStatus::Completed,
            Some(json!({"exit_code": 0, "stdout": "test output"})),
        )
        .await;

    // The completed operation should be in completion history
    let completed_ops = operation_monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 1);
    assert_eq!(completed_ops[0].id, "test-op-123");
    assert_eq!(completed_ops[0].state, OperationStatus::Completed);

    // Multiple fetches should return the same operation (persistent)
    let second_fetch = operation_monitor.get_completed_operations().await;
    assert_eq!(second_fetch.len(), 1);
    assert_eq!(second_fetch[0].id, "test-op-123");
}

#[tokio::test]
async fn test_multiple_operations_tracked_in_history() {
    // Test that multiple completed operations are tracked in persistent history

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    // Add and complete multiple operations
    for i in 1..=3 {
        let operation = Operation::new(
            format!("test-op-{}", i),
            "test".to_string(),
            format!("Test operation {}", i),
            None,
        );
        operation_monitor.add_operation(operation).await;

        operation_monitor
            .update_status(
                &format!("test-op-{}", i),
                OperationStatus::Completed,
                Some(json!({"exit_code": 0, "stdout": format!("output {}", i)})),
            )
            .await;
    }

    // All 3 operations should be in completion history
    let completed_ops = operation_monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 3);

    let ids: HashSet<String> = completed_ops.iter().map(|op| op.id.clone()).collect();
    assert!(ids.contains("test-op-1"));
    assert!(ids.contains("test-op-2"));
    assert!(ids.contains("test-op-3"));
}

#[tokio::test]
async fn test_notification_loop_simulation() {
    // Simulate the notification loop to verify persistent history works correctly

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    // Add a pending operation first
    let op_id = "loop-test-456";
    let operation = Operation::new(
        op_id.to_string(),
        "test".to_string(),
        "Loop test operation".to_string(),
        None,
    );
    operation_monitor.add_operation(operation).await;

    // Complete it via update_status (this moves it to completion_history)
    operation_monitor
        .update_status(
            op_id,
            OperationStatus::Completed,
            Some(json!({"exit_code": 0, "stdout": "loop test output"})),
        )
        .await;

    // Wait for the operation to be completed
    operation_monitor.wait_for_operation(op_id).await;

    let mut total_operations_found = 0;

    // Simulate notification loop - should find the operation consistently in history
    for _iteration in 1..=5 {
        let completed_ops = operation_monitor.get_completed_operations().await;

        if !completed_ops.is_empty() {
            total_operations_found += completed_ops.len();
        }
    }

    // Operation should be found in completion history every time (5 iterations)
    // because it persists in completion_history. The MCP callback system
    // handles deduplication to prevent actual duplicate notifications.
    assert_eq!(
        total_operations_found, 5,
        "Operation should be found in completion history every time (5 iterations), but was found {} times",
        total_operations_found
    );
}

#[tokio::test]
async fn test_continuous_updates_dont_duplicate_history() {
    // Test that update_status called repeatedly for a completed operation
    // doesn't create duplicates in the history.

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    let op_id = "continuous-update-test";

    // Add and complete operation
    let operation = Operation::new(
        op_id.to_string(),
        "test".to_string(),
        "continuous update test".to_string(),
        None,
    );
    operation_monitor.add_operation(operation).await;
    operation_monitor
        .update_status(
            op_id,
            OperationStatus::Completed,
            Some(Value::String("initial completion".to_string())),
        )
        .await;

    // Check the history
    let initial_history = operation_monitor.get_completed_operations().await;
    assert_eq!(
        initial_history.len(),
        1,
        "Should be one operation in history"
    );

    // Now continuously try to update the status
    for i in 1..=5 {
        operation_monitor
            .update_status(
                op_id,
                OperationStatus::Completed,
                Some(Value::String(format!("update attempt {}", i))),
            )
            .await;

        // Check that the history still only contains one entry for this op_id
        let current_history = operation_monitor.get_completed_operations().await;
        assert_eq!(
            current_history.len(),
            1,
            "Status update {} caused history to have {} operations, expected 1!",
            i,
            current_history.len()
        );
    }

    // Final verification
    let final_history = operation_monitor.get_completed_operations().await;
    assert_eq!(
        final_history.len(),
        1,
        "Final check should still find exactly one operation"
    );
}

#[tokio::test]
async fn test_continuous_add_operations() {
    // Test if there's an issue with multiple add_operation calls for the same ID

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    let op_id = "continuous-add-test";

    // Add operation multiple times (simulate potential race conditions)
    for i in 1..=3 {
        let operation = Operation::new(
            op_id.to_string(),
            "test".to_string(),
            format!("continuous add test {}", i),
            None,
        );
        operation_monitor.add_operation(operation).await;
    }

    // Complete the operation
    operation_monitor
        .update_status(
            op_id,
            OperationStatus::Completed,
            Some(Value::String("completed".to_string())),
        )
        .await;

    // Check how many completed operations there are
    let completed = operation_monitor.get_completed_operations().await;

    // Should be exactly 1, not 3
    assert_eq!(
        completed.len(),
        1,
        "Expected 1 completed operation, found {}. Multiple add_operation calls might be causing duplicates.",
        completed.len()
    );
}
