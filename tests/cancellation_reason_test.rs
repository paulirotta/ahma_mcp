use ahma_mcp::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
use std::time::Duration;

#[tokio::test]
async fn test_cancel_operation_with_reason_persists_reason() {
    let monitor = OperationMonitor::new(MonitorConfig::with_timeout(Duration::from_secs(5)));
    let op_id = "cancel_reason_op".to_string();
    let op = Operation::new(
        op_id.clone(),
        "test_tool".to_string(),
        "A test operation".to_string(),
        None,
    );

    monitor.add_operation(op).await;

    // Cancel with an explicit reason
    let reason = Some("LLM requested to stop long-running test".to_string());
    let cancelled = monitor
        .cancel_operation_with_reason(&op_id, reason.clone())
        .await;
    assert!(cancelled);

    // Should no longer be in active ops; check completion history for reason
    let completed = monitor.get_completed_operations().await;
    let found = completed.into_iter().find(|o| o.id == op_id).unwrap();
    assert_eq!(found.state, OperationStatus::Cancelled);

    let result = found.result.expect("cancel result should be present");
    let reason_str = result.get("reason").and_then(|v| v.as_str()).unwrap_or("");
    assert!(reason_str.contains("LLM requested"));
}

/// This is a scaffolding test to ensure shutdown timeout is at least 120s and that
/// the MonitorConfig carries that value forward. Full integration of signal-driven
/// shutdown is covered by runtime tests.
#[tokio::test]
async fn test_monitor_shutdown_timeout_is_120s() {
    let cfg = MonitorConfig::with_timeout(Duration::from_secs(1));
    assert_eq!(cfg.shutdown_timeout.as_secs(), 120);
}
