use ahma_core::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
use std::time::Duration;
use tokio::time::sleep;

use std::sync::Arc;

#[tokio::test]
async fn test_operation_timeout_enforcement() {
    // Create a monitor with a default timeout
    let config = MonitorConfig::with_timeout(Duration::from_secs(5));
    let monitor = Arc::new(OperationMonitor::new(config));

    // Start the background monitor
    OperationMonitor::start_background_monitor(monitor.clone());

    // Create an operation with a short timeout (100ms)
    let op_id = "timeout_test_op".to_string();
    let op = Operation::new_with_timeout(
        op_id.clone(),
        "test_tool".to_string(),
        "Test operation".to_string(),
        None,
        Some(Duration::from_millis(100)),
    );

    monitor.add_operation(op).await;

    // Set status to InProgress
    monitor
        .update_status(&op_id, OperationStatus::InProgress, None)
        .await;

    // Wait for longer than the timeout and the monitor check interval (1s)
    sleep(Duration::from_millis(1500)).await;

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
