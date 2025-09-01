#[cfg(test)]
mod endless_notification_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_completed_operations_are_cleared_after_notification() {
        // FAILING TEST: Verify that get_and_clear_completed_operations()
        // actually clears operations and prevents endless loops

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Add a completed operation
        let operation = Operation {
            id: "test-op-123".to_string(),
            tool_name: "test".to_string(),
            description: "Test operation".to_string(),
            state: OperationStatus::Completed,
            result: Some(json!({"exit_code": 0, "stdout": "test output"})),
        };

        operation_monitor.add_operation(operation).await;

        // First call should return the completed operation
        let first_fetch = operation_monitor.get_and_clear_completed_operations().await;
        assert_eq!(first_fetch.len(), 1);
        assert_eq!(first_fetch[0].id, "test-op-123");
        println!("✓ First fetch returned completed operation");

        // Second call should return empty (operation was cleared)
        let second_fetch = operation_monitor.get_and_clear_completed_operations().await;
        assert_eq!(
            second_fetch.len(),
            0,
            "BUG: Operation should have been cleared after first fetch, but {} operations were returned",
            second_fetch.len()
        );
        println!("✓ Second fetch returned no operations - cleared properly");

        // Verify that regular get_completed_operations also shows empty
        let check_remaining = operation_monitor.get_completed_operations().await;
        assert_eq!(
            check_remaining.len(),
            0,
            "BUG: get_completed_operations() still shows {} operations after clearing",
            check_remaining.len()
        );
        println!("✓ No operations remain in monitor after clearing");
    }

    #[tokio::test]
    async fn test_multiple_operations_cleared_correctly() {
        // Test that multiple completed operations are all cleared properly

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Add multiple completed operations
        for i in 1..=3 {
            let operation = Operation {
                id: format!("test-op-{}", i),
                tool_name: "test".to_string(),
                description: format!("Test operation {}", i),
                state: OperationStatus::Completed,
                result: Some(json!({"exit_code": 0, "stdout": format!("output {}", i)})),
            };
            operation_monitor.add_operation(operation).await;
        }

        // First fetch should get all 3 operations
        let first_fetch = operation_monitor.get_and_clear_completed_operations().await;
        assert_eq!(first_fetch.len(), 3);
        println!("✓ First fetch returned {} operations", first_fetch.len());

        // Second fetch should be empty
        let second_fetch = operation_monitor.get_and_clear_completed_operations().await;
        assert_eq!(
            second_fetch.len(),
            0,
            "BUG: All operations should have been cleared, but {} remained",
            second_fetch.len()
        );
        println!("✓ All operations properly cleared");
    }

    #[tokio::test]
    async fn test_notification_loop_simulation() {
        // Simulate the notification loop to verify no endless notifications

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Add a completed operation
        let operation = Operation {
            id: "loop-test-456".to_string(),
            tool_name: "test".to_string(),
            description: "Loop test operation".to_string(),
            state: OperationStatus::Completed,
            result: Some(json!({"exit_code": 0, "stdout": "loop test output"})),
        };
        operation_monitor.add_operation(operation).await;

        let mut notification_count = 0;

        // Simulate notification loop - should only notify once
        for iteration in 1..=5 {
            let completed_ops = operation_monitor.get_and_clear_completed_operations().await;

            if !completed_ops.is_empty() {
                notification_count += completed_ops.len();
                println!(
                    "Iteration {}: Found {} operations to notify",
                    iteration,
                    completed_ops.len()
                );

                // Simulate sending notifications (what main.rs does)
                for op in &completed_ops {
                    println!("  Notifying operation: {}", op.id);
                }
            } else {
                println!("Iteration {}: No operations to notify", iteration);
            }

            sleep(Duration::from_millis(100)).await; // Brief delay like the real loop
        }

        // We should have only notified once for the single operation
        assert_eq!(
            notification_count, 1,
            "BUG: Operation should only be notified once, but was notified {} times",
            notification_count
        );
        println!("✓ Operation was only notified once - no endless loop");
    }
}
