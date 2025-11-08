/// Manual test to verify await functionality
/// Run this with: cargo test await_manual_verification
use std::time::Instant;

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

    // Simulate operation taking 2 seconds
    let monitor_clone = monitor.clone();
    let op_id_clone = op_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        monitor_clone
            .update_status(
                &op_id_clone,
                OperationStatus::Completed,
                Some(serde_json::json!("Operation completed")),
            )
            .await;
    });

    // Test await functionality - this should block until completion
    let start = Instant::now();
    let _result = monitor.wait_for_operation(&op_id).await;
    let duration = start.elapsed();

    println!("Await took: {:?}", duration);

    // Verify it blocked for at least 1.5 seconds
    assert!(
        duration.as_secs_f64() >= 1.5,
        "Await should have blocked for at least 1.5 seconds, but returned in {:?}",
        duration
    );

    println!("✅ Await fix verification passed!");
}
