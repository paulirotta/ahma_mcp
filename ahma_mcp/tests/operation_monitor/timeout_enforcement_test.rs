use ahma_mcp::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
use std::time::{Duration, SystemTime};

use std::sync::Arc;

#[tokio::test]
async fn test_operation_timeout_enforcement() {
    // Create a monitor with a default timeout
    let config = MonitorConfig::with_timeout(Duration::from_secs(5));
    let monitor = Arc::new(OperationMonitor::new(config));

    // Create an operation with a short timeout (100ms)
    let op_id = "timeout_test_op".to_string();
    let mut op = Operation::new_with_timeout(
        op_id.clone(),
        "test_tool".to_string(),
        "Test operation".to_string(),
        None,
        Some(Duration::from_millis(100)),
    );
    // Force the operation to appear old enough to timeout immediately
    op.start_time = SystemTime::now() - Duration::from_millis(200);

    monitor.add_operation(op).await;

    // Set status to InProgress
    monitor
        .update_status(&op_id, OperationStatus::InProgress, None)
        .await;

    // Trigger timeout check without real waiting
    monitor.check_timeouts().await;

    // Check status
    let op = monitor
        .wait_for_operation(&op_id)
        .await
        .expect("Operation should exist");

    // This assertion is expected to fail until we implement the background monitor
    assert_eq!(
        op.state,
        OperationStatus::TimedOut,
        "Operation should have timed out, but state is {:?}",
        op.state
    );
}
