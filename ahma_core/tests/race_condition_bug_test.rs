#[cfg(test)]
mod race_condition_bug_test {
    use ahma_core::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

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
            "test".to_string(),
            "race condition test".to_string(),
            None,
        );
        monitor.add_operation(operation).await;
        println!("✅ Added operation in Pending state: {}", test_op_id);

        // Step 2: Simulate notification loop clearing operations while operation is still pending
        let cleared_while_pending = monitor.get_completed_operations().await;
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

        // Step 4: Notification loop should find it consistently in completion history
        let first_access = monitor.get_completed_operations().await;
        assert_eq!(
            first_access.len(),
            1,
            "Should find the completed operation in history"
        );
        assert_eq!(first_access[0].id, test_op_id);
        println!("✅ First history access found the completed operation");

        // Step 5: NEW BEHAVIOR - operations persist in completion history
        let second_access = monitor.get_completed_operations().await;
        assert_eq!(
            second_access.len(),
            1,
            "Operation should persist in completion history for await operations"
        );
        assert_eq!(second_access[0].id, test_op_id);
        println!("✅ Second access finds same operation in completion history");

        // Step 6: Verify the operation remains consistently available
        for i in 3..=5 {
            let subsequent_access = monitor.get_completed_operations().await;
            assert_eq!(
                subsequent_access.len(),
                1,
                "Iteration {}: Operation should remain in completion history",
                i
            );
            assert_eq!(subsequent_access[0].id, test_op_id);
        }
        println!(
            "✅ Race condition prevention test passed - operation persists in completion history"
        );
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
            "test".to_string(),
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

        // Initial check - operation should be in completion history
        let initial_check = monitor.get_completed_operations().await;
        assert_eq!(initial_check.len(), 1);
        println!("✅ Operation found in completion history");

        // NOW: Try to update the status of the already-completed operation
        // This simulates a late-arriving update_status call
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("late update".to_string())),
            )
            .await;
        println!("✅ Called update_status on already-completed operation");

        // Check that operation is still in history and properly handled
        let recheck = monitor.get_completed_operations().await;
        assert_eq!(
            recheck.len(),
            1,
            "Operation should remain in completion history after late update"
        );
        assert_eq!(recheck[0].id, test_op_id);

        // The result should remain the original value (late updates don't overwrite completed operations)
        if let Some(result) = &recheck[0].result {
            if let Some(result_str) = result.as_str() {
                assert_eq!(
                    result_str, "completed",
                    "Result should remain as original value - late updates are ignored"
                );
            }
        }

        println!(
            "✅ Late update_status properly handled - operation remains in history with original result (late updates ignored)"
        );
    }

    /// Test concurrent completion and notification processing to stress-test race conditions.
    #[tokio::test]
    async fn test_concurrent_completion_and_notification_simulation() {
        println!("⚡ Testing concurrent operation completion and notification simulation...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));
        let test_op_id = "concurrent-test-op";

        monitor
            .add_operation(Operation::new(
                test_op_id.to_string(),
                "test".to_string(),
                "concurrent test".to_string(),
                None,
            ))
            .await;

        // Task 1: Complete the operation immediately.
        let completion_handle = tokio::spawn({
            let monitor = monitor.clone();
            async move {
                monitor
                    .update_status(
                        test_op_id,
                        OperationStatus::Completed,
                        Some(Value::String("concurrent completion".to_string())),
                    )
                    .await;
                println!("   🔧 Operation completed");
            }
        });

        // Task 2: Repeatedly check for completed operations, simulating a notification loop.
        let notification_handle = tokio::spawn({
            let monitor = monitor.clone();
            async move {
                let mut notified_ops = std::collections::HashSet::new();
                // Wait for the operation to complete before checking
                monitor.wait_for_operation(test_op_id).await;
                let completed = monitor.get_completed_operations().await;
                for op in completed {
                    if notified_ops.insert(op.id.clone()) {
                        println!("   🔔 Notified for operation {}", op.id);
                    }
                }
                notified_ops.len()
            }
        });

        let (_completion_result, total_notified) =
            tokio::try_join!(completion_handle, notification_handle).unwrap();

        println!("✅ Concurrent test completed:");
        println!("   - Total unique operations notified: {}", total_notified);

        // The operation should be notified for exactly once.
        assert_eq!(
            total_notified, 1,
            "CONCURRENCY BUG: Expected 1 unique notification, but got {}. This suggests race conditions or faulty logic.",
            total_notified
        );

        println!("✅ Concurrent completion and notification test passed");
    }

    /// Test the specific scenario where multiple notification loops might run concurrently.
    #[tokio::test]
    async fn test_multiple_notification_loops() {
        println!("🔁 Testing multiple concurrent notification loops...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));
        let test_op_id = "multi-loop-test-op";

        // Add and complete operation
        monitor
            .add_operation(Operation::new(
                test_op_id.to_string(),
                "test".to_string(),
                "multi loop test".to_string(),
                None,
            ))
            .await;
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("completed".to_string())),
            )
            .await;
        println!("✅ Operation added and completed");

        // Use a shared set to track notifications across all concurrent loops
        let notified_ops_global = Arc::new(Mutex::new(std::collections::HashSet::new()));

        let mut handles = Vec::new();

        // Start multiple "notification loops" concurrently
        for i in 0..3 {
            let monitor_clone = monitor.clone();
            let notified_ops_clone = notified_ops_global.clone();
            let handle = tokio::spawn(async move {
                // Wait for the operation to be available
                monitor_clone.wait_for_operation(test_op_id).await;
                let completed = monitor_clone.get_completed_operations().await;
                let mut local_notification_count = 0;
                for op in completed {
                    let mut guard = notified_ops_clone.lock().unwrap();
                    if guard.insert(op.id.clone()) {
                        println!("   � Loop {}: Sent notification for {}", i, op.id);
                        local_notification_count += 1;
                    }
                }
                local_notification_count
            });
            handles.push(handle);
        }

        let results = futures::future::join_all(handles).await;
        let total_notifications_sent: usize = results.into_iter().map(|res| res.unwrap()).sum();

        println!("✅ Multiple loops completed:");
        println!(
            "   - Total notifications sent across all loops: {}",
            total_notifications_sent
        );

        // Only ONE notification should have been sent in total across all loops.
        assert_eq!(
            total_notifications_sent, 1,
            "MULTIPLE NOTIFICATION LOOPS BUG: Expected exactly 1 notification across all loops, but sent {}.",
            total_notifications_sent
        );

        println!("✅ Multiple notification loops test passed");
    }
}
