#[cfg(test)]
mod endless_notification_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::json;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_completed_operations_are_persistent_in_history() {
        // NEW TEST: Verify that completed operations are moved to completion_history
        // and remain accessible for wait operations while preventing duplicates

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Add an operation and complete it via update_status
        let operation = Operation {
            id: "test-op-123".to_string(),
            tool_name: "test".to_string(),
            description: "Test operation".to_string(),
            state: OperationStatus::Pending,
            result: None,
        };

        operation_monitor.add_operation(operation).await;

        // Complete the operation (this should move it to completion_history)
        operation_monitor.update_status(
            "test-op-123",
            OperationStatus::Completed,
            Some(json!({"exit_code": 0, "stdout": "test output"}))
        ).await;

        // The completed operation should be in completion history
        let completed_ops = operation_monitor.get_completed_operations().await;
        assert_eq!(completed_ops.len(), 1);
        assert_eq!(completed_ops[0].id, "test-op-123");
        assert_eq!(completed_ops[0].state, OperationStatus::Completed);
        println!("✓ Completed operation is in completion history");

        // Multiple fetches should return the same operation (persistent)
        let second_fetch = operation_monitor.get_completed_operations().await;
        assert_eq!(second_fetch.len(), 1);
        assert_eq!(second_fetch[0].id, "test-op-123");
        println!("✓ Operation remains in history after multiple fetches");
    }

    #[tokio::test]
    async fn test_multiple_operations_tracked_in_history() {
        // Test that multiple completed operations are tracked in persistent history

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Add and complete multiple operations
        for i in 1..=3 {
            let operation = Operation {
                id: format!("test-op-{}", i),
                tool_name: "test".to_string(),
                description: format!("Test operation {}", i),
                state: OperationStatus::Pending,
                result: None,
            };
            operation_monitor.add_operation(operation).await;
            
            operation_monitor.update_status(
                &format!("test-op-{}", i),
                OperationStatus::Completed,
                Some(json!({"exit_code": 0, "stdout": format!("output {}", i)}))
            ).await;
        }

        // All 3 operations should be in completion history
        let completed_ops = operation_monitor.get_completed_operations().await;
        assert_eq!(completed_ops.len(), 3);
        
        let ids: HashSet<String> = completed_ops.iter().map(|op| op.id.clone()).collect();
        assert!(ids.contains("test-op-1"));
        assert!(ids.contains("test-op-2"));
        assert!(ids.contains("test-op-3"));
        
        println!("✓ All {} operations are in completion history", completed_ops.len());
    }

    #[tokio::test]
    async fn test_notification_loop_simulation() {
        // Simulate the notification loop to verify persistent history works correctly

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Add a pending operation first
        let operation = Operation {
            id: "loop-test-456".to_string(),
            tool_name: "test".to_string(),
            description: "Loop test operation".to_string(),
            state: OperationStatus::Pending,
            result: None,
        };
        operation_monitor.add_operation(operation).await;

        // Complete it via update_status (this moves it to completion_history)
        operation_monitor.update_status(
            "loop-test-456",
            OperationStatus::Completed,
            Some(json!({"exit_code": 0, "stdout": "loop test output"}))
        ).await;

        let mut total_operations_found = 0;

        // Simulate notification loop - should find the operation consistently in history  
        for iteration in 1..=5 {
            let completed_ops = operation_monitor.get_completed_operations().await;

            if !completed_ops.is_empty() {
                total_operations_found += completed_ops.len();
                println!(
                    "Iteration {}: Found {} operations in completion history",
                    iteration,
                    completed_ops.len()
                );

                // Simulate sending notifications (what main.rs would do)
                for op in &completed_ops {
                    println!("  Operation in history: {}", op.id);
                }
            } else {
                println!("Iteration {}: No operations in completion history", iteration);
            }

            sleep(Duration::from_millis(100)).await; // Brief delay like the real loop
        }

        // NEW BEHAVIOR: We should find the operation in every iteration (5 times)
        // because it persists in completion_history, but the MCP callback system
        // handles deduplication to prevent actual duplicate notifications
        assert_eq!(
            total_operations_found, 5,
            "BUG: Operation should be found in completion history every time (5 iterations), but was found {} times",
            total_operations_found
        );
        println!("✓ Operation found in completion history consistently (MCP handles notification deduplication)");
    }
}
