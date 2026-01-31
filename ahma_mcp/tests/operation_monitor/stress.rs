//! Stress tests for operation monitor async behavior and performance
//! Tests concurrent operations, rapid-fire calls, error resilience, and timeout handling

use ahma_mcp::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
use anyhow::Result;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_operation_monitor_concurrent_operations() -> Result<()> {
    // Test that operation monitor can handle multiple concurrent operations
    let config = MonitorConfig {
        default_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(60),
    };

    let monitor = OperationMonitor::new(config);
    let mut handles = Vec::new();

    // Create 10 concurrent operations
    for i in 0..10 {
        let monitor_clone = monitor.clone();
        let op_id = format!("concurrent_op_{}", i);

        let handle = tokio::spawn(async move {
            let operation = Operation::new(
                op_id.clone(),
                "test_tool".to_string(),
                format!("Concurrent operation #{}", i),
                None, // result is None initially
            );

            // Register the operation
            monitor_clone.add_operation(operation).await;

            // Simulate some work without time-based sleeps
            tokio::task::yield_now().await;

            // Complete the operation using update_status
            let result = serde_json::json!({"result": format!("completed_{}", i)});
            monitor_clone
                .update_status(&op_id, OperationStatus::Completed, Some(result))
                .await;

            op_id
        });

        handles.push(handle);
    }

    // Wait for all operations to complete
    let results = futures::future::join_all(handles).await;

    // Verify all operations completed successfully
    for result in results {
        let op_id = result?;

        // Check completion history since completed operations get moved there
        let completed_ops = monitor.get_completed_operations().await;
        let found = completed_ops.iter().find(|op| op.id == op_id);
        assert!(
            found.is_some(),
            "Operation {} should exist in completed operations",
            op_id
        );
        assert_eq!(found.unwrap().state, OperationStatus::Completed);
        assert!(found.unwrap().result.is_some());
    }

    println!("Successfully completed 10 concurrent operations");
    Ok(())
}

#[tokio::test]
async fn test_operation_monitor_rapid_fire_operations() -> Result<()> {
    // Test that operation monitor can handle rapid-fire operation creation
    let config = MonitorConfig {
        default_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(60),
    };

    let monitor = OperationMonitor::new(config);
    const NUM_OPERATIONS: usize = 50; // Reduced from 100 for speed

    let start = Instant::now();

    // Create operations rapidly
    for i in 0..NUM_OPERATIONS {
        let op_id = format!("rapid_fire_{}", i);
        let operation = Operation::new(
            op_id.clone(),
            "test_tool".to_string(),
            format!("Rapid fire operation #{}", i),
            None,
        );

        monitor.add_operation(operation).await;

        // Complete immediately using update_status
        let result = serde_json::json!({"iteration": i});
        monitor
            .update_status(&op_id, OperationStatus::Completed, Some(result))
            .await;
    }

    let duration = start.elapsed();
    println!("Completed {} operations in {:?}", NUM_OPERATIONS, duration);

    // Performance check - should handle rapid operations efficiently
    assert!(
        duration < Duration::from_secs(5),
        "Rapid fire operations took too long: {:?}",
        duration
    );

    // Verify all operations completed - they should be in completion history
    let completed_ops = monitor.get_completed_operations().await;
    assert_eq!(completed_ops.len(), NUM_OPERATIONS);

    for i in 0..NUM_OPERATIONS {
        let op_id = format!("rapid_fire_{}", i);
        let found = completed_ops.iter().find(|op| op.id == op_id);
        assert!(
            found.is_some(),
            "Operation {} should exist in completed operations",
            op_id
        );
        assert_eq!(found.unwrap().state, OperationStatus::Completed);
    }

    Ok(())
}

#[tokio::test]
async fn test_operation_monitor_mixed_operations() -> Result<()> {
    // Test that operation monitor handles mixed success/failure scenarios
    let config = MonitorConfig {
        default_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(60),
    };

    let monitor = OperationMonitor::new(config);

    // Create a failing operation
    let failing_op = Operation::new(
        "failing_operation".to_string(),
        "test_tool".to_string(),
        "This operation will fail".to_string(),
        None,
    );
    monitor.add_operation(failing_op).await;

    // Create a successful operation
    let success_op = Operation::new(
        "success_operation".to_string(),
        "test_tool".to_string(),
        "This operation will succeed".to_string(),
        None,
    );
    monitor.add_operation(success_op).await;

    // Fail the first operation using update_status
    let error_result = serde_json::json!({
        "error": "Simulated failure",
        "exit_code": 1
    });
    monitor
        .update_status(
            "failing_operation",
            OperationStatus::Failed,
            Some(error_result),
        )
        .await;

    // Complete the second operation
    let success_result = serde_json::json!({"status": "success"});
    monitor
        .update_status(
            "success_operation",
            OperationStatus::Completed,
            Some(success_result),
        )
        .await;

    // Verify states in completion history
    let completed_ops = monitor.get_completed_operations().await;

    let failed_op = completed_ops.iter().find(|op| op.id == "failing_operation");
    assert!(
        failed_op.is_some(),
        "Failed operation should exist in completion history"
    );
    assert_eq!(failed_op.unwrap().state, OperationStatus::Failed);
    assert!(failed_op.unwrap().result.is_some());

    let successful_op = completed_ops.iter().find(|op| op.id == "success_operation");
    assert!(
        successful_op.is_some(),
        "Successful operation should exist in completion history"
    );
    assert_eq!(successful_op.unwrap().state, OperationStatus::Completed);
    assert!(successful_op.unwrap().result.is_some());

    println!("Mixed operations test passed - handled both success and failure correctly");
    Ok(())
}

#[tokio::test]
async fn test_operation_monitor_basic_functionality() -> Result<()> {
    // Test basic operation monitor functionality
    let config = MonitorConfig {
        default_timeout: Duration::from_secs(30),
        shutdown_timeout: Duration::from_secs(60),
    };

    let monitor = OperationMonitor::new(config);

    // Create an operation
    let operation = Operation::new(
        "basic_test".to_string(),
        "test_tool".to_string(),
        "Basic functionality test".to_string(),
        None,
    );
    monitor.add_operation(operation).await;

    // Verify it's in active operations
    let active_ops = monitor.get_active_operations().await;
    let active_op = active_ops.iter().find(|op| op.id == "basic_test");
    assert!(
        active_op.is_some(),
        "Operation should be in active operations"
    );
    assert_eq!(active_op.unwrap().state, OperationStatus::Pending);

    // Complete the operation
    let result = serde_json::json!({"status": "basic_test_completed"});
    monitor
        .update_status("basic_test", OperationStatus::Completed, Some(result))
        .await;

    // Verify it's moved to completed operations
    let completed_ops = monitor.get_completed_operations().await;
    let completed_op = completed_ops.iter().find(|op| op.id == "basic_test");
    assert!(
        completed_op.is_some(),
        "Operation should be in completed operations"
    );
    assert_eq!(completed_op.unwrap().state, OperationStatus::Completed);
    assert!(completed_op.unwrap().result.is_some());

    // Verify it's no longer in active operations
    let active_ops_after = monitor.get_active_operations().await;
    let active_op_after = active_ops_after.iter().find(|op| op.id == "basic_test");
    assert!(
        active_op_after.is_none(),
        "Operation should no longer be in active operations"
    );

    println!("Basic functionality test passed");
    Ok(())
}
