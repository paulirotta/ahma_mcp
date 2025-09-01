#[cfg(test)]
mod race_condition_bug_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    /// This test specifically targets the potential race condition that could cause
    /// the endless notification loop bug. The scenario:
    /// 1. Operation is added to monitor
    /// 2. Notification loop clears operations (but operation isn't completed yet)
    /// 3. Operation completes and calls update_status AFTER clearing
    /// 4. Next notification iteration finds the "new" completed operation
    /// 5. This creates a scenario where the same operation keeps being found
    #[tokio::test]
    async fn test_race_condition_between_completion_and_clearing() {
        println!("🏁 Testing race condition between operation completion and clearing...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "race-condition-test-op";

        // Step 1: Add operation in Pending state (simulating adapter.execute_async_in_dir)
        let operation = Operation::new(
            test_op_id.to_string(),
            "race condition test".to_string(),
            None,
        );
        monitor.add_operation(operation).await;
        println!("✅ Added operation in Pending state: {}", test_op_id);

        // Step 2: Simulate notification loop clearing operations while operation is still pending
        let cleared_while_pending = monitor.get_and_clear_completed_operations().await;
        assert!(
            cleared_while_pending.is_empty(),
            "Should not clear pending operations"
        );
        println!("✅ Notification loop found no completed operations (expected)");

        // Step 3: Now the operation completes (simulating the tokio::spawn completion)
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("race condition test completed".to_string())),
            )
            .await;
        println!("✅ Operation completed and status updated");

        // Step 4: Next notification loop iteration should find it
        let first_clear = monitor.get_and_clear_completed_operations().await;
        assert_eq!(first_clear.len(), 1, "Should find the completed operation");
        assert_eq!(first_clear[0].id, test_op_id);
        println!("✅ First notification clear found the completed operation");

        // Step 5: THE CRITICAL TEST - subsequent clears should be empty
        let second_clear = monitor.get_and_clear_completed_operations().await;
        if !second_clear.is_empty() {
            panic!(
                "RACE CONDITION BUG DETECTED: Operation {} was found again after being cleared! This suggests the operation was re-added or not properly removed.",
                test_op_id
            );
        }
        println!("✅ Second clear is empty - no race condition detected");

        // Step 6: Verify the operation is completely gone
        for i in 3..=5 {
            let subsequent_clear = monitor.get_and_clear_completed_operations().await;
            if !subsequent_clear.is_empty() {
                panic!(
                    "PERSISTENT RACE CONDITION: Operation found again in iteration {}",
                    i
                );
            }
        }
        println!("✅ Race condition test passed");
    }

    /// Test what happens if update_status is called AFTER the operation is cleared
    #[tokio::test]
    async fn test_update_status_after_clear_race_condition() {
        println!("🔄 Testing update_status called after operation is cleared...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "update-after-clear-test";

        // Add and immediately complete operation
        let operation = Operation::new(
            test_op_id.to_string(),
            "update after clear test".to_string(),
            None,
        );
        monitor.add_operation(operation).await;
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("completed".to_string())),
            )
            .await;

        // Clear the completed operation
        let cleared = monitor.get_and_clear_completed_operations().await;
        assert_eq!(cleared.len(), 1);
        println!("✅ Operation cleared");

        // NOW: Try to update the status of the already-cleared operation
        // This simulates a late-arriving update_status call
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("late update".to_string())),
            )
            .await;
        println!("✅ Called update_status on already-cleared operation");

        // Check if this causes the operation to reappear
        let recheck = monitor.get_and_clear_completed_operations().await;
        if !recheck.is_empty() {
            panic!(
                "BUG DETECTED: update_status on cleared operation caused it to reappear! Found {} operations",
                recheck.len()
            );
        }
        println!("✅ No operations reappeared - update_status after clear is safe");
    }

    /// Test concurrent completion and clearing to stress-test race conditions  
    #[tokio::test]
    async fn test_concurrent_completion_and_clearing() {
        println!("⚡ Testing concurrent operation completion and clearing...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "concurrent-test-op";

        // Add operation
        let operation = Operation::new(test_op_id.to_string(), "concurrent test".to_string(), None);
        monitor.add_operation(operation).await;

        // Start concurrent tasks
        let monitor_for_completion = monitor.clone();
        let monitor_for_clearing = monitor.clone();

        // Task 1: Complete the operation after a delay
        let completion_handle = tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            monitor_for_completion
                .update_status(
                    test_op_id,
                    OperationStatus::Completed,
                    Some(Value::String("concurrent completion".to_string())),
                )
                .await;
            println!("   🔧 Operation completed");
        });

        // Task 2: Repeatedly try to clear operations (simulating notification loop)
        let clearing_handle = tokio::spawn(async move {
            let mut total_found = 0;
            let mut iterations = 0;

            for _ in 0..10 {
                // 10 iterations over 1 second
                sleep(Duration::from_millis(100)).await;
                let cleared = monitor_for_clearing
                    .get_and_clear_completed_operations()
                    .await;
                total_found += cleared.len();
                iterations += 1;

                if !cleared.is_empty() {
                    println!(
                        "   🧹 Clearing iteration {}: found {} operations",
                        iterations,
                        cleared.len()
                    );
                    for op in &cleared {
                        println!("      - Cleared operation: {}", op.id);
                    }
                }
            }

            (total_found, iterations)
        });

        // Wait for both tasks to complete
        let (_completion_result, (total_cleared, clearing_iterations)) =
            tokio::try_join!(completion_handle, clearing_handle).unwrap();

        println!("✅ Concurrent test completed:");
        println!("   - Total operations cleared: {}", total_cleared);
        println!("   - Clearing iterations: {}", clearing_iterations);

        // The operation should be cleared exactly once
        if total_cleared != 1 {
            panic!(
                "CONCURRENCY BUG: Expected 1 operation to be cleared, but {} were cleared. This suggests race conditions causing duplicate notifications.",
                total_cleared
            );
        }

        // Final verification - no operations should remain
        let final_check = monitor.get_and_clear_completed_operations().await;
        assert!(
            final_check.is_empty(),
            "No operations should remain after concurrent test"
        );

        println!("✅ Concurrent completion and clearing test passed");
    }

    /// Test the specific scenario where multiple notification loops might run
    #[tokio::test]
    async fn test_multiple_notification_loops() {
        println!("🔁 Testing multiple concurrent notification loops...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "multi-loop-test-op";

        // Add and complete operation
        let operation = Operation::new(test_op_id.to_string(), "multi loop test".to_string(), None);
        monitor.add_operation(operation).await;
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("completed".to_string())),
            )
            .await;

        println!("✅ Operation added and completed");

        // Start multiple "notification loops" concurrently
        let monitor1 = monitor.clone();
        let monitor2 = monitor.clone();
        let monitor3 = monitor.clone();

        let loop1 = tokio::spawn(async move {
            let mut found_ops = Vec::new();
            for i in 1..=3 {
                sleep(Duration::from_millis(100)).await;
                let ops = monitor1.get_and_clear_completed_operations().await;
                if !ops.is_empty() {
                    println!(
                        "   📊 Loop 1, iteration {}: found {} operations",
                        i,
                        ops.len()
                    );
                    found_ops.extend(ops);
                }
            }
            found_ops
        });

        let loop2 = tokio::spawn(async move {
            let mut found_ops = Vec::new();
            for i in 1..=3 {
                sleep(Duration::from_millis(150)).await; // Slightly different timing
                let ops = monitor2.get_and_clear_completed_operations().await;
                if !ops.is_empty() {
                    println!(
                        "   📊 Loop 2, iteration {}: found {} operations",
                        i,
                        ops.len()
                    );
                    found_ops.extend(ops);
                }
            }
            found_ops
        });

        let loop3 = tokio::spawn(async move {
            let mut found_ops = Vec::new();
            for i in 1..=3 {
                sleep(Duration::from_millis(75)).await; // Different timing again
                let ops = monitor3.get_and_clear_completed_operations().await;
                if !ops.is_empty() {
                    println!(
                        "   📊 Loop 3, iteration {}: found {} operations",
                        i,
                        ops.len()
                    );
                    found_ops.extend(ops);
                }
            }
            found_ops
        });

        let (loop1_ops, loop2_ops, loop3_ops) = tokio::try_join!(loop1, loop2, loop3).unwrap();

        let total_operations_found = loop1_ops.len() + loop2_ops.len() + loop3_ops.len();

        println!("✅ Multiple loops completed:");
        println!("   - Loop 1 found: {} operations", loop1_ops.len());
        println!("   - Loop 2 found: {} operations", loop2_ops.len());
        println!("   - Loop 3 found: {} operations", loop3_ops.len());
        println!("   - Total found: {}", total_operations_found);

        // Only ONE of the loops should find the operation
        if total_operations_found != 1 {
            panic!(
                "MULTIPLE NOTIFICATION LOOPS BUG: Expected exactly 1 operation across all loops, but found {}. This suggests the same operation was notified multiple times.",
                total_operations_found
            );
        }

        println!("✅ Multiple notification loops test passed");
    }
}
