#[cfg(test)]
mod continuous_update_test {
    use ahma_core::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;

    /// This test simulates a scenario where update_status is called repeatedly
    /// for a completed operation, ensuring it doesn't create duplicates in the history.
    #[tokio::test]
    async fn test_continuous_updates_dont_duplicate_history() {
        println!("🔄 Testing continuous status updates for completed operations...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        let op_id = "continuous-update-test";

        // Add and complete operation
        let operation = Operation::new(
            op_id.to_string(),
            "test".to_string(),
            "continuous update test".to_string(),
            None,
        );
        operation_monitor.add_operation(operation).await;
        operation_monitor
            .update_status(
                op_id,
                OperationStatus::Completed,
                Some(Value::String("initial completion".to_string())),
            )
            .await;

        println!("✅ Added and completed operation");

        // Check the history
        let initial_history = operation_monitor.get_completed_operations().await;
        assert_eq!(
            initial_history.len(),
            1,
            "Should be one operation in history"
        );
        println!(
            "✅ Verified initial history (found {} operations)",
            initial_history.len()
        );

        // Now continuously try to update the status
        for i in 1..=5 {
            operation_monitor
                .update_status(
                    op_id,
                    OperationStatus::Completed,
                    Some(Value::String(format!("update attempt {}", i))),
                )
                .await;

            println!("📝 Status update attempt {}", i);

            // Check that the history still only contains one entry for this op_id
            let current_history = operation_monitor.get_completed_operations().await;
            if current_history.len() != 1 {
                panic!(
                    "BUG: Status update {} caused history to have {} operations, expected 1!",
                    i,
                    current_history.len()
                );
            }
        }

        println!("✅ No duplicate operations appeared in history after continuous status updates");

        // Final verification
        let final_history = operation_monitor.get_completed_operations().await;
        assert_eq!(
            final_history.len(),
            1,
            "Final check should still find exactly one operation"
        );

        println!("✅ Test passed - continuous updates don't cause history duplication");
    }

    /// Test if there's an issue with multiple add_operation calls for the same ID
    #[tokio::test]
    async fn test_continuous_add_operations() {
        println!("🔄 Testing continuous add_operation calls with same ID...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        let op_id = "continuous-add-test";

        // Add operation multiple times (simulate potential race conditions)
        for i in 1..=3 {
            let operation = Operation::new(
                op_id.to_string(),
                "test".to_string(),
                format!("continuous add test {}", i),
                None,
            );
            operation_monitor.add_operation(operation).await;
            println!("📝 Add operation attempt {}", i);
        }

        // Complete the operation
        operation_monitor
            .update_status(
                op_id,
                OperationStatus::Completed,
                Some(Value::String("completed".to_string())),
            )
            .await;

        // Check how many completed operations there are
        let completed = operation_monitor.get_completed_operations().await;
        println!("📊 Found {} completed operations", completed.len());

        // Should be exactly 1, not 3
        assert_eq!(
            completed.len(),
            1,
            "Expected 1 completed operation, found {}. Multiple add_operation calls might be causing duplicates.",
            completed.len()
        );

        println!("✅ Test passed - multiple add_operation calls don't cause duplicates in history");
    }
}
