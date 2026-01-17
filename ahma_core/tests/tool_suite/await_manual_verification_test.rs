/// Manual test to verify await functionality
/// Run this with: cargo test await_manual_verification
#[tokio::test]
async fn await_manual_verification() {
    use ahma_core::{
        adapter::Adapter,
        operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus},
        shell_pool::{ShellPoolConfig, ShellPoolManager},
    };
    use std::{sync::Arc, time::Duration};

    // Set up the monitor directly
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
    let _adapter = Arc::new(Adapter::new(monitor.clone(), shell_pool).unwrap());

    println!("Testing await fix directly using the operation monitor...");

    // Create a mock long-running operation manually
    let op_id = "test_operation_123".to_string();
    let operation = Operation::new(
        op_id.clone(),
        "test_tool".to_string(),
        "Testing await fix".to_string(),
        None,
    );

    println!("Started operation: {}", op_id);

    // Add the operation to the monitor
    monitor.add_operation(operation).await;

    // Complete operation without time-based sleep
    let monitor_clone = monitor.clone();
    let op_id_clone = op_id.clone();
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        monitor_clone
            .update_status(
                &op_id_clone,
                OperationStatus::Completed,
                Some(serde_json::json!("Operation completed")),
            )
            .await;
    });

    // Test await functionality - this should block until completion
    let result = monitor.wait_for_operation(&op_id).await;
    assert!(result.is_some());
    assert_eq!(result.unwrap().state, OperationStatus::Completed);

    println!("✅ Await fix verification passed!");
}
