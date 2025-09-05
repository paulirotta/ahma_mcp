use ahma_mcp::operation_monitor::{
    MonitorConfig, Operation, OperationMonitor, OperationStatus, ShutdownSummary,
};
use std::time::{Duration, SystemTime};

#[test]
fn test_operation_status_is_terminal() {
    assert!(!OperationStatus::Pending.is_terminal());
    assert!(!OperationStatus::InProgress.is_terminal());
    assert!(OperationStatus::Completed.is_terminal());
    assert!(OperationStatus::Failed.is_terminal());
    assert!(OperationStatus::Cancelled.is_terminal());
    assert!(OperationStatus::TimedOut.is_terminal());
}

#[test]
fn test_operation_status_equality() {
    assert_eq!(OperationStatus::Pending, OperationStatus::Pending);
    assert_eq!(OperationStatus::InProgress, OperationStatus::InProgress);
    assert_eq!(OperationStatus::Completed, OperationStatus::Completed);
    assert_eq!(OperationStatus::Failed, OperationStatus::Failed);
    assert_eq!(OperationStatus::Cancelled, OperationStatus::Cancelled);
    assert_eq!(OperationStatus::TimedOut, OperationStatus::TimedOut);

    assert_ne!(OperationStatus::Pending, OperationStatus::InProgress);
    assert_ne!(OperationStatus::Completed, OperationStatus::Failed);
}

#[test]
fn test_operation_status_serialization() {
    let status = OperationStatus::InProgress;
    let json = serde_json::to_string(&status).unwrap();
    let deserialized: OperationStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(status, deserialized);

    // Test all variants
    let statuses = vec![
        OperationStatus::Pending,
        OperationStatus::InProgress,
        OperationStatus::Completed,
        OperationStatus::Failed,
        OperationStatus::Cancelled,
        OperationStatus::TimedOut,
    ];

    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: OperationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}

#[test]
fn test_operation_new() {
    let operation = Operation::new(
        "test_op_123".to_string(),
        "cargo".to_string(),
        "Build project".to_string(),
        Some(serde_json::json!({"status": "success"})),
    );

    assert_eq!(operation.id, "test_op_123");
    assert_eq!(operation.tool_name, "cargo");
    assert_eq!(operation.description, "Build project");
    assert_eq!(operation.state, OperationStatus::Pending);
    assert!(operation.result.is_some());
    assert!(operation.end_time.is_none());
    assert!(operation.first_wait_time.is_none());
}

#[test]
fn test_operation_new_without_result() {
    let operation = Operation::new(
        "test_op_456".to_string(),
        "git".to_string(),
        "Commit changes".to_string(),
        None,
    );

    assert_eq!(operation.id, "test_op_456");
    assert_eq!(operation.tool_name, "git");
    assert_eq!(operation.description, "Commit changes");
    assert_eq!(operation.state, OperationStatus::Pending);
    assert!(operation.result.is_none());
}

#[test]
fn test_operation_serialization() {
    let mut operation = Operation::new(
        "serial_test".to_string(),
        "test_tool".to_string(),
        "Test operation".to_string(),
        Some(serde_json::json!({"data": "test"})),
    );
    operation.state = OperationStatus::Completed;
    operation.end_time = Some(SystemTime::now());

    let json = serde_json::to_string(&operation).unwrap();
    let deserialized: Operation = serde_json::from_str(&json).unwrap();

    assert_eq!(operation.id, deserialized.id);
    assert_eq!(operation.tool_name, deserialized.tool_name);
    assert_eq!(operation.description, deserialized.description);
    assert_eq!(operation.state, deserialized.state);
    assert_eq!(operation.result, deserialized.result);
    // Note: cancellation_token is skipped in serialization
}

#[test]
fn test_monitor_config_with_timeout() {
    let timeout = Duration::from_secs(120);
    let config = MonitorConfig::with_timeout(timeout);

    assert_eq!(config.default_timeout, timeout);
    assert_eq!(config.shutdown_timeout, Duration::from_secs(120)); // Default shutdown timeout
}

#[test]
fn test_monitor_config_with_timeouts() {
    let operation_timeout = Duration::from_secs(300);
    let shutdown_timeout = Duration::from_secs(30);
    let config = MonitorConfig::with_timeouts(operation_timeout, shutdown_timeout);

    assert_eq!(config.default_timeout, operation_timeout);
    assert_eq!(config.shutdown_timeout, shutdown_timeout);
}

#[test]
fn test_shutdown_summary_fields() {
    let operations = vec![
        Operation::new(
            "op1".to_string(),
            "tool1".to_string(),
            "desc1".to_string(),
            None,
        ),
        Operation::new(
            "op2".to_string(),
            "tool2".to_string(),
            "desc2".to_string(),
            None,
        ),
    ];

    let summary = ShutdownSummary {
        total_active: 2,
        operations: operations.clone(),
    };

    assert_eq!(summary.total_active, 2);
    assert_eq!(summary.operations.len(), 2);
    assert_eq!(summary.operations[0].id, "op1");
    assert_eq!(summary.operations[1].id, "op2");
}

#[tokio::test]
async fn test_operation_monitor_new() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    // Should start with no operations
    let operations = monitor.get_all_operations().await;
    assert!(operations.is_empty());
}

#[tokio::test]
async fn test_operation_monitor_add_and_get() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let operation = Operation::new(
        "test_add".to_string(),
        "cargo".to_string(),
        "Test add operation".to_string(),
        None,
    );

    monitor.add_operation(operation.clone()).await;

    let retrieved = monitor.get_operation("test_add").await;
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "test_add");
    assert_eq!(retrieved.tool_name, "cargo");
}

#[tokio::test]
async fn test_operation_monitor_get_nonexistent() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let result = monitor.get_operation("nonexistent").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_operation_monitor_get_all_operations() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let op1 = Operation::new(
        "op1".to_string(),
        "tool1".to_string(),
        "desc1".to_string(),
        None,
    );
    let op2 = Operation::new(
        "op2".to_string(),
        "tool2".to_string(),
        "desc2".to_string(),
        None,
    );

    monitor.add_operation(op1).await;
    monitor.add_operation(op2).await;

    let all_ops = monitor.get_all_operations().await;
    assert_eq!(all_ops.len(), 2);

    let ids: Vec<&str> = all_ops.iter().map(|op| op.id.as_str()).collect();
    assert!(ids.contains(&"op1"));
    assert!(ids.contains(&"op2"));
}

#[tokio::test]
async fn test_operation_monitor_update_status() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let operation = Operation::new(
        "status_test".to_string(),
        "cargo".to_string(),
        "Status test".to_string(),
        None,
    );

    monitor.add_operation(operation).await;

    // Update status to InProgress
    monitor
        .update_status("status_test", OperationStatus::InProgress, None)
        .await;

    let updated = monitor.get_operation("status_test").await.unwrap();
    assert_eq!(updated.state, OperationStatus::InProgress);

    // Update status to Completed with result
    let result = serde_json::json!({"success": true});
    monitor
        .update_status(
            "status_test",
            OperationStatus::Completed,
            Some(result.clone()),
        )
        .await;

    // After completion, the operation is moved to completion history
    let completed_ops = monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 1);
    let completed = &completed_ops[0];
    assert_eq!(completed.state, OperationStatus::Completed);
    assert_eq!(completed.result, Some(result));
    assert!(completed.end_time.is_some());
}

#[tokio::test]
async fn test_operation_monitor_cancel_operation() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let operation = Operation::new(
        "cancel_test".to_string(),
        "cargo".to_string(),
        "Cancel test".to_string(),
        None,
    );

    monitor.add_operation(operation).await;

    let cancelled = monitor.cancel_operation("cancel_test").await;
    assert!(cancelled);

    // After cancellation, the operation is moved to completion history
    let operation = monitor.get_operation("cancel_test").await;
    assert!(operation.is_none()); // Should no longer be in active operations

    // Check the completed operations instead
    let completed_ops = monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 1);
    let cancelled_op = &completed_ops[0];
    assert_eq!(cancelled_op.id, "cancel_test");
    assert_eq!(cancelled_op.state, OperationStatus::Cancelled);
    assert!(cancelled_op.cancellation_token.is_cancelled());
}

#[tokio::test]
async fn test_operation_monitor_cancel_nonexistent() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let cancelled = monitor.cancel_operation("nonexistent").await;
    assert!(!cancelled);
}

#[tokio::test]
async fn test_operation_monitor_get_active_operations() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let op1 = Operation::new(
        "active1".to_string(),
        "tool1".to_string(),
        "desc1".to_string(),
        None,
    );
    let mut op2 = Operation::new(
        "completed1".to_string(),
        "tool2".to_string(),
        "desc2".to_string(),
        None,
    );
    op2.state = OperationStatus::Completed;

    monitor.add_operation(op1).await;
    monitor.add_operation(op2).await;

    let active_ops = monitor.get_active_operations().await;
    assert_eq!(active_ops.len(), 1);
    assert_eq!(active_ops[0].id, "active1");
}

#[tokio::test]
async fn test_operation_monitor_get_completed_operations() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    // Add an operation and complete it
    let operation = Operation::new(
        "completion_test".to_string(),
        "cargo".to_string(),
        "Completion test".to_string(),
        None,
    );

    monitor.add_operation(operation).await;
    monitor
        .update_status("completion_test", OperationStatus::Completed, None)
        .await;

    let completed_ops = monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 1);
    assert_eq!(completed_ops[0].id, "completion_test");
    assert_eq!(completed_ops[0].state, OperationStatus::Completed);
}

#[tokio::test]
async fn test_operation_monitor_get_shutdown_summary() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let op1 = Operation::new(
        "active1".to_string(),
        "tool1".to_string(),
        "desc1".to_string(),
        None,
    );
    let op2 = Operation::new(
        "active2".to_string(),
        "tool2".to_string(),
        "desc2".to_string(),
        None,
    );

    monitor.add_operation(op1).await;
    monitor.add_operation(op2).await;

    let summary = monitor.get_shutdown_summary().await;
    assert_eq!(summary.total_active, 2);
    assert_eq!(summary.operations.len(), 2);
}

#[tokio::test]
async fn test_operation_monitor_wait_for_operation_completed() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    // Add a completed operation
    let mut operation = Operation::new(
        "wait_test".to_string(),
        "cargo".to_string(),
        "Wait test".to_string(),
        Some(serde_json::json!({"result": "success"})),
    );
    operation.state = OperationStatus::Completed;

    monitor.add_operation(operation).await;

    let result = monitor.wait_for_operation("wait_test").await;
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.state, OperationStatus::Completed);
    assert!(result.first_wait_time.is_some());
}

#[tokio::test]
async fn test_operation_monitor_wait_for_operation_nonexistent() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let result = monitor.wait_for_operation("nonexistent").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_operation_monitor_concurrent_access() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let monitor_clone1 = monitor.clone();
    let monitor_clone2 = monitor.clone();

    let task1 = tokio::spawn(async move {
        let op = Operation::new(
            "concurrent1".to_string(),
            "tool1".to_string(),
            "desc1".to_string(),
            None,
        );
        monitor_clone1.add_operation(op).await;
    });

    let task2 = tokio::spawn(async move {
        let op = Operation::new(
            "concurrent2".to_string(),
            "tool2".to_string(),
            "desc2".to_string(),
            None,
        );
        monitor_clone2.add_operation(op).await;
    });

    let _ = tokio::join!(task1, task2);

    let all_ops = monitor.get_all_operations().await;
    assert_eq!(all_ops.len(), 2);
}

#[tokio::test]
async fn test_operation_state_transitions() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let operation = Operation::new(
        "transition_test".to_string(),
        "cargo".to_string(),
        "State transition test".to_string(),
        None,
    );

    monitor.add_operation(operation).await;

    // Verify initial state
    let op = monitor.get_operation("transition_test").await.unwrap();
    assert_eq!(op.state, OperationStatus::Pending);

    // Transition to InProgress
    monitor
        .update_status("transition_test", OperationStatus::InProgress, None)
        .await;
    let op = monitor.get_operation("transition_test").await.unwrap();
    assert_eq!(op.state, OperationStatus::InProgress);
    assert!(op.end_time.is_none());

    // Transition to Completed
    monitor
        .update_status(
            "transition_test",
            OperationStatus::Completed,
            Some(serde_json::json!({"done": true})),
        )
        .await;

    // After completion, the operation is moved to completion history
    let completed_ops = monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 1);
    let op = &completed_ops[0];
    assert_eq!(op.state, OperationStatus::Completed);
    assert!(op.end_time.is_some());
    assert_eq!(op.result, Some(serde_json::json!({"done": true})));
}

#[test]
fn test_operation_status_debug() {
    let status = OperationStatus::InProgress;
    let debug_str = format!("{:?}", status);
    assert!(debug_str.contains("InProgress"));
}

#[test]
fn test_operation_debug() {
    let operation = Operation::new(
        "debug_test".to_string(),
        "test_tool".to_string(),
        "Debug test".to_string(),
        None,
    );

    let debug_str = format!("{:?}", operation);
    assert!(debug_str.contains("debug_test"));
    assert!(debug_str.contains("test_tool"));
    assert!(debug_str.contains("Debug test"));
}

#[test]
fn test_monitor_config_debug() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(120));
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("MonitorConfig"));
}

#[test]
fn test_operation_clone() {
    let operation = Operation::new(
        "clone_test".to_string(),
        "test_tool".to_string(),
        "Clone test".to_string(),
        Some(serde_json::json!({"data": "test"})),
    );

    let cloned = operation.clone();
    assert_eq!(operation.id, cloned.id);
    assert_eq!(operation.tool_name, cloned.tool_name);
    assert_eq!(operation.description, cloned.description);
    assert_eq!(operation.state, cloned.state);
    assert_eq!(operation.result, cloned.result);
}

#[tokio::test]
async fn test_operation_monitor_multiple_status_updates() {
    let config = MonitorConfig::with_timeout(Duration::from_secs(60));
    let monitor = OperationMonitor::new(config);

    let operation = Operation::new(
        "multi_update".to_string(),
        "cargo".to_string(),
        "Multiple updates test".to_string(),
        None,
    );

    monitor.add_operation(operation).await;

    // Multiple rapid updates
    monitor
        .update_status("multi_update", OperationStatus::InProgress, None)
        .await;
    monitor
        .update_status(
            "multi_update",
            OperationStatus::InProgress,
            Some(serde_json::json!({"progress": 50})),
        )
        .await;
    monitor
        .update_status(
            "multi_update",
            OperationStatus::Completed,
            Some(serde_json::json!({"progress": 100})),
        )
        .await;

    // After completion, check in completion history
    let completed_ops = monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), 1);
    let final_op = &completed_ops[0];
    assert_eq!(final_op.state, OperationStatus::Completed);
    assert_eq!(final_op.result, Some(serde_json::json!({"progress": 100})));
}

#[tokio::test]
async fn test_cancellation_token_behavior() {
    let operation = Operation::new(
        "token_test".to_string(),
        "test".to_string(),
        "Token test".to_string(),
        None,
    );

    assert!(!operation.cancellation_token.is_cancelled());

    operation.cancellation_token.cancel();
    assert!(operation.cancellation_token.is_cancelled());
}
