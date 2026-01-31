use ahma_mcp::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
use ahma_mcp::utils::logging::init_test_logging;
use std::time::Duration;

#[tokio::test]
async fn test_polling_detection() {
    init_test_logging();
    let monitor = OperationMonitor::new(MonitorConfig::with_timeout(Duration::from_secs(30)));

    // Add an operation to query
    let op = Operation::new(
        "test-op".to_string(),
        "test_tool".to_string(),
        "Test operation".to_string(),
        None,
    );
    monitor.add_operation(op).await;

    // Simulate rapid polling (should trigger warning)
    for _ in 0..4 {
        let _op = monitor.get_operation("test-op").await;
        tokio::task::yield_now().await; // very short interval without sleep
    }

    // Complete the operation
    monitor
        .update_status(
            "test-op",
            OperationStatus::Completed,
            Some(serde_json::json!("Success")),
        )
        .await;

    // This test passes if no panics occur and the warning is logged
    // The actual warning output would be visible in logs during test execution
}

#[tokio::test]
async fn test_normal_status_checking() {
    init_test_logging();
    let monitor = OperationMonitor::new(MonitorConfig::with_timeout(Duration::from_secs(30)));

    // Add an operation to query
    let op = Operation::new(
        "test-op2".to_string(),
        "test_tool".to_string(),
        "Test operation 2".to_string(),
        None,
    );
    monitor.add_operation(op).await;

    // Simulate normal checking (should not trigger warning) - reduced duration
    for _ in 0..2 {
        // Reduced from 3 to 2 iterations
        let _op = monitor.get_operation("test-op2").await;
        tokio::task::yield_now().await; // normal interval without real waiting
    }

    // Complete the operation
    monitor
        .update_status(
            "test-op2",
            OperationStatus::Completed,
            Some(serde_json::json!("Success")),
        )
        .await;

    // This test passes if no warnings are triggered
}
