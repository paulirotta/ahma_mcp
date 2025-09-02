#[cfg(test)]
mod endless_notification_bug_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    /// This test reproduces the exact endless notification loop bug described in the user's logs.
    /// The bug: operation "7fc7f85e-9217-4f48-9240-2dd0dcd908e0" keeps appearing in notifications
    /// every second even after being supposedly cleared.
    #[tokio::test]
    async fn test_endless_notification_loop_bug_reproduction() {
        println!("🐛 Testing endless notification loop bug reproduction...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        // Use a test operation ID
        let problematic_op_id = "op_test_endless";

        // Add a completed operation (simulating what happens when command finishes)
        let operation = Operation::new(
            problematic_op_id.to_string(),
            "cargo".to_string(),
            "cargo version".to_string(),
            Some(Value::String("completed".to_string())),
        );
        monitor.add_operation(operation).await;

        // Update it to completed status (this is what the adapter does)
        monitor
            .update_status(
                problematic_op_id,
                OperationStatus::Completed,
                Some(Value::String("cargo 1.89.0".to_string())),
            )
            .await;

        println!("✅ Added and completed operation: {}", problematic_op_id);

        // Simulate the notification loop behavior - this should clear the operation
        let first_batch = monitor.get_completed_operations().await;
        assert_eq!(first_batch.len(), 1, "Should find the completed operation");
        assert_eq!(first_batch[0].id, problematic_op_id);
        println!("✅ First call to get_and_clear_completed_operations found 1 operation");

        // The bug: subsequent calls should return empty, but if the bug exists,
        // they will keep returning the same operation
        let second_batch = monitor.get_completed_operations().await;

        // THIS ASSERTION SHOULD PASS if the fix works, but may FAIL if the bug exists
        if !second_batch.is_empty() {
            println!(
                "🐛 BUG DETECTED: Second batch found {} operations when it should be empty!",
                second_batch.len()
            );
            for op in &second_batch {
                println!("   - Operation {} still present", op.id);
            }
            // This is the failing assertion that proves the bug
            panic!(
                "ENDLESS NOTIFICATION LOOP BUG: Operation {} was returned again after being cleared!",
                problematic_op_id
            );
        } else {
            println!("✅ Second batch is empty - operation was properly cleared");
        }

        // Simulate multiple notification loop iterations (like the 1-second interval)
        for i in 3..=6 {
            sleep(Duration::from_millis(50)).await; // Small delay to simulate real timing
            let batch = monitor.get_completed_operations().await;
            if !batch.is_empty() {
                panic!(
                    "ENDLESS NOTIFICATION LOOP BUG: Iteration {} found {} operations when list should be empty!",
                    i,
                    batch.len()
                );
            }
        }

        println!("✅ Test passed - no endless notification loop detected");
    }

    /// Test that simulates race condition where operations might be getting re-added
    #[tokio::test]
    async fn test_concurrent_operation_clearing() {
        println!("🔀 Testing concurrent operation clearing...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "concurrent-test-op-123";

        // Add operation
        let operation = Operation::new(
            test_op_id.to_string(),
            "test".to_string(),
            "test command".to_string(),
            None,
        );
        monitor.add_operation(operation).await;

        // Mark as completed
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("success".to_string())),
            )
            .await;

        // Simulate multiple concurrent notification loops trying to clear the same operation
        let monitor1 = monitor.clone();
        let monitor2 = monitor.clone();
        let monitor3 = monitor.clone();

        let handle1 =
            tokio::spawn(async move { monitor1.get_completed_operations().await });

        let handle2 =
            tokio::spawn(async move { monitor2.get_completed_operations().await });

        let handle3 =
            tokio::spawn(async move { monitor3.get_completed_operations().await });

        let results = tokio::try_join!(handle1, handle2, handle3).unwrap();

        let total_operations_found = results.0.len() + results.1.len() + results.2.len();

        // Only ONE of the concurrent calls should find the operation
        if total_operations_found > 1 {
            panic!(
                "RACE CONDITION BUG: Expected 1 total operation across all concurrent calls, but found {}",
                total_operations_found
            );
        }

        // Verify the operation is truly gone
        let final_check = monitor.get_completed_operations().await;
        assert!(
            final_check.is_empty(),
            "Operation should be completely cleared after concurrent access"
        );

        println!("✅ Concurrent clearing test passed");
    }

    /// Test that reproduces the exact timing from user logs (notifications every 1 second)
    #[tokio::test]
    async fn test_one_second_interval_notification_loop() {
        println!("⏰ Testing 1-second interval notification loop...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "timing-test-op-456";

        // Add and complete operation
        let operation = Operation::new(
            test_op_id.to_string(),
            "test".to_string(),
            "timing test".to_string(),
            None,
        );
        monitor.add_operation(operation).await;
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("timing success".to_string())),
            )
            .await;

        // First notification loop iteration - should find the operation
        let first_iteration = monitor.get_completed_operations().await;
        assert_eq!(first_iteration.len(), 1);
        println!("✅ Iteration 1: Found {} operations", first_iteration.len());

        // Simulate the next 5 seconds of 1-second intervals
        for iteration in 2..=6 {
            sleep(Duration::from_secs(1)).await; // Exact timing from real bug
            let batch = monitor.get_completed_operations().await;

            if !batch.is_empty() {
                panic!(
                    "ENDLESS NOTIFICATION BUG: Iteration {} at {}s found {} operations. This matches the user's log pattern of notifications every second!",
                    iteration,
                    iteration,
                    batch.len()
                );
            }
            println!(
                "✅ Iteration {}: Found {} operations (expected 0)",
                iteration,
                batch.len()
            );
        }

        println!("✅ 1-second interval test passed - no endless notifications");
    }
}
