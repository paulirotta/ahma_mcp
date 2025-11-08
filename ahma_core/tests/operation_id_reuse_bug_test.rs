#[cfg(test)]
mod operation_id_reuse_bug_test {
    use ahma_core::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;

    /// This test verifies that if an operation ID is reused, the `OperationMonitor`
    /// correctly overwrites the entry in the completion history, preventing duplicates.
    #[tokio::test]
    async fn test_duplicate_operation_id_scenario() {
        println!("🆔 Testing duplicate operation ID scenario...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));
        let op_id = "op_test_1";

        // First operation instance
        let operation1 = Operation::new(
            op_id.to_string(),
            "test".to_string(),
            "first operation".to_string(),
            None,
        );
        monitor.add_operation(operation1).await;
        monitor
            .update_status(
                op_id,
                OperationStatus::Completed,
                Some(Value::String("first completion".to_string())),
            )
            .await;

        let history1 = monitor.get_completed_operations().await;
        assert_eq!(
            history1.len(),
            1,
            "History should have one entry after first completion."
        );
        println!("✅ First operation completed and is in history.");

        // Second operation instance with the same ID
        let operation2 = Operation::new(
            op_id.to_string(),
            "test".to_string(),
            "second operation with same ID".to_string(),
            None,
        );
        monitor.add_operation(operation2).await;
        println!("⚠️  Added second operation with same ID (should overwrite).");

        monitor
            .update_status(
                op_id,
                OperationStatus::Completed,
                Some(Value::String("second completion".to_string())),
            )
            .await;

        // The history should still contain only one entry for this ID.
        let history2 = monitor.get_completed_operations().await;
        assert_eq!(
            history2.len(),
            1,
            "History should still have only one entry after second completion."
        );

        let final_op = history2.first().unwrap();
        assert_eq!(
            final_op.description, "second operation with same ID",
            "The operation should have been overwritten with the new description."
        );

        println!("✅ Operation ID reuse test passed: History was correctly overwritten.");
    }

    /// Verifies that multiple `add_operation` calls for the same ID before completion
    /// result in only a single entry in the completion history.
    #[tokio::test]
    async fn test_multiple_add_operation_calls() {
        println!("➕ Testing multiple add_operation calls with same ID...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));
        let test_op_id = "multi-add-test-op";

        // Add operation multiple times, simulating a race or redundant calls
        monitor
            .add_operation(Operation::new(
                test_op_id.to_string(),
                "test".to_string(),
                "first add".to_string(),
                None,
            ))
            .await;
        monitor
            .add_operation(Operation::new(
                test_op_id.to_string(),
                "test".to_string(),
                "second add".to_string(),
                None,
            ))
            .await;
        println!("✅ Called add_operation multiple times.");

        // Complete the operation
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("completion".to_string())),
            )
            .await;

        // There should be exactly one entry in the history.
        let completed = monitor.get_completed_operations().await;
        assert_eq!(
            completed.len(),
            1,
            "Expected 1 completed operation, found {}. Multiple add_operation calls caused duplicates.",
            completed.len()
        );

        println!(
            "✅ Multiple add_operation test passed: Only one operation was recorded in history."
        );
    }

    /// Simulates a realistic production sequence to ensure notifications are not sent endlessly.
    #[tokio::test]
    async fn test_production_like_sequence() {
        println!("🏭 Testing production-like sequence...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));
        let test_op_id = "production-sequence-test";
        let mut notified_ops = std::collections::HashSet::new();

        // Step 1 & 2: Operation is added and becomes in-progress
        monitor
            .add_operation(Operation::new(
                test_op_id.to_string(),
                "cargo".to_string(),
                "cargo version".to_string(),
                None,
            ))
            .await;
        monitor
            .update_status(test_op_id, OperationStatus::InProgress, None)
            .await;
        println!("✅ Step 1-2: Operation added and in-progress.");

        // Step 3: Notification loop runs, finds no completed operations
        let completed1 = monitor.get_completed_operations().await;
        assert!(
            completed1.is_empty(),
            "No operations should be complete yet."
        );
        println!("✅ Step 3: Notification loop finds no completed operations.");

        // Step 4: Operation completes
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("cargo 1.89.0".to_string())),
            )
            .await;
        println!("✅ Step 4: Operation completed.");

        // Step 5: Notification loop finds the completed operation
        let completed2 = monitor.get_completed_operations().await;
        let mut new_notifications = 0;
        for op in completed2 {
            if notified_ops.insert(op.id.clone()) {
                new_notifications += 1;
            }
        }
        assert_eq!(
            new_notifications, 1,
            "Should have sent exactly one notification."
        );
        println!("✅ Step 5: Notification loop processed 1 new operation.");

        // Step 6-10: Subsequent notification loops should find the same history but send no new notifications
        for step in 6..=10 {
            let completed3 = monitor.get_completed_operations().await;
            let mut subsequent_notifications = 0;
            for op in completed3 {
                if notified_ops.insert(op.id.clone()) {
                    subsequent_notifications += 1;
                }
            }
            assert_eq!(
                subsequent_notifications, 0,
                "PRODUCTION SEQUENCE BUG: Step {}: Sent a duplicate notification!",
                step
            );
            println!(
                "✅ Step {}: Notification loop sent 0 new notifications (expected).",
                step
            );
        }

        println!("✅ Production sequence test passed.");
    }

    /// Tests edge cases around status transitions, ensuring the completion history remains consistent.
    #[tokio::test]
    async fn test_status_transition_edge_cases() {
        println!("🔄 Testing status transition edge cases...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));
        let test_op_id = "status-transition-test";

        // Add operation
        monitor
            .add_operation(Operation::new(
                test_op_id.to_string(),
                "test".to_string(),
                "status transition test".to_string(),
                None,
            ))
            .await;

        // Update status to Completed multiple times
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("first completion".to_string())),
            )
            .await;
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("second completion".to_string())),
            )
            .await;
        println!("✅ Updated status to Completed multiple times.");

        // The history should only contain one entry.
        let completed = monitor.get_completed_operations().await;
        assert_eq!(
            completed.len(),
            1,
            "STATUS TRANSITION BUG: Expected 1 completed operation, found {}. Multiple status updates created duplicates.",
            completed.len()
        );

        // The operation is now in history. Further updates should be ignored because it's no longer in the active `operations` map.
        monitor
            .update_status(
                test_op_id,
                OperationStatus::Completed,
                Some(Value::String("post-history completion".to_string())),
            )
            .await;
        println!("✅ Attempted post-history status update (should be ignored).");

        // The history should still have only one entry.
        let final_history = monitor.get_completed_operations().await;
        assert_eq!(
            final_history.len(),
            1,
            "POST-HISTORY UPDATE BUG: History size changed after update to an already-completed operation."
        );

        println!("✅ Status transition edge cases test passed.");
    }
}
