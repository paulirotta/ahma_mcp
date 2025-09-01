#[cfg(test)]
mod operation_id_reuse_bug_test {
    use ahma_mcp::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;

    /// Test what happens if the same operation ID is used multiple times
    /// This could happen if UUIDs are somehow reused or if there are multiple calls
    #[tokio::test]
    async fn test_duplicate_operation_id_scenario() {
        println!("🆔 Testing duplicate operation ID scenario...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        // This is the exact operation ID from the user's logs
        let problematic_op_id = "7fc7f85e-9217-4f48-9240-2dd0dcd908e0";

        println!("🔍 Testing with problematic operation ID: {}", problematic_op_id);

        // Scenario: First operation with this ID
        let operation1 = Operation::new(
            problematic_op_id.to_string(),
            "first operation".to_string(),
            None,
        );
        monitor.add_operation(operation1).await;
        
        // Complete it
        monitor.update_status(
            problematic_op_id,
            OperationStatus::Completed,
            Some(Value::String("first completion".to_string())),
        ).await;

        // Clear it (simulating notification loop)
        let first_clear = monitor.get_and_clear_completed_operations().await;
        assert_eq!(first_clear.len(), 1);
        println!("✅ First operation cleared");

        // Scenario: What if the SAME operation ID is used again?
        // This shouldn't happen with proper UUIDs, but let's test it
        let operation2 = Operation::new(
            problematic_op_id.to_string(),
            "second operation with same ID".to_string(),
            None,
        );
        monitor.add_operation(operation2).await;
        println!("⚠️  Added second operation with same ID");

        // Complete the second operation
        monitor.update_status(
            problematic_op_id,
            OperationStatus::Completed,
            Some(Value::String("second completion".to_string())),
        ).await;

        // Now check - this could cause endless notifications if the ID reuse creates issues
        let second_clear = monitor.get_and_clear_completed_operations().await;
        assert_eq!(second_clear.len(), 1);
        println!("✅ Second operation cleared");

        // The critical test - no operations should remain
        let final_check = monitor.get_and_clear_completed_operations().await;
        if !final_check.is_empty() {
            panic!("OPERATION ID REUSE BUG: Found {} operations after clearing. This suggests operation ID reuse is causing endless notifications.", final_check.len());
        }

        println!("✅ Operation ID reuse test passed");
    }

    /// Test what happens if add_operation is called multiple times with same ID
    #[tokio::test]
    async fn test_multiple_add_operation_calls() {
        println!("➕ Testing multiple add_operation calls with same ID...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "multi-add-test-op";

        // Add operation first time
        let operation1 = Operation::new(
            test_op_id.to_string(),
            "first add".to_string(),
            None,
        );
        monitor.add_operation(operation1).await;
        println!("✅ First add_operation call");

        // Add operation AGAIN with same ID - this overwrites the first one
        let operation2 = Operation::new(
            test_op_id.to_string(),
            "second add".to_string(),
            None,
        );
        monitor.add_operation(operation2).await;
        println!("✅ Second add_operation call (should overwrite)");

        // Complete the operation
        monitor.update_status(
            test_op_id,
            OperationStatus::Completed,
            Some(Value::String("completion".to_string())),
        ).await;

        // Check how many completed operations there are
        let completed = monitor.get_completed_operations().await;
        println!("📊 Found {} completed operations", completed.len());
        
        // Should be exactly 1, not 2
        if completed.len() != 1 {
            panic!("MULTIPLE ADD BUG: Expected 1 completed operation, found {}. Multiple add_operation calls may be causing duplicates.", completed.len());
        }

        // Clear and verify
        let cleared = monitor.get_and_clear_completed_operations().await;
        assert_eq!(cleared.len(), 1);
        
        // Ensure it's really gone
        let final_check = monitor.get_and_clear_completed_operations().await;
        assert!(final_check.is_empty());

        println!("✅ Multiple add_operation test passed");
    }

    /// Test the exact sequence that might be happening in production
    #[tokio::test]
    async fn test_production_like_sequence() {
        println!("🏭 Testing production-like sequence...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "production-sequence-test";

        // Step 1: Operation is added (from execute_async_in_dir)
        let operation = Operation::new(
            test_op_id.to_string(),
            "cargo version".to_string(),
            None,
        );
        monitor.add_operation(operation).await;
        println!("✅ Step 1: Operation added");

        // Step 2: Operation is marked as InProgress
        monitor.update_status(
            test_op_id,
            OperationStatus::InProgress,
            None,
        ).await;
        println!("✅ Step 2: Operation marked as InProgress");

        // Step 3: Notification loop runs but finds no completed operations
        let check1 = monitor.get_and_clear_completed_operations().await;
        assert!(check1.is_empty());
        println!("✅ Step 3: Notification loop finds no completed operations");

        // Step 4: Operation completes
        monitor.update_status(
            test_op_id,
            OperationStatus::Completed,
            Some(Value::String("cargo 1.89.0".to_string())),
        ).await;
        println!("✅ Step 4: Operation completed");

        // Step 5: Notification loop finds and clears the completed operation
        let check2 = monitor.get_and_clear_completed_operations().await;
        assert_eq!(check2.len(), 1);
        println!("✅ Step 5: Notification loop finds and clears 1 operation");

        // Step 6-10: Subsequent notification loops should find nothing
        for step in 6..=10 {
            let check = monitor.get_and_clear_completed_operations().await;
            if !check.is_empty() {
                panic!("PRODUCTION SEQUENCE BUG: Step {}: Found {} operations when none expected. This matches the user's endless notification pattern!", step, check.len());
            }
            println!("✅ Step {}: Notification loop finds 0 operations (expected)", step);
        }

        println!("✅ Production sequence test passed");
    }

    /// Test if there's an issue with the exact operation status transitions
    #[tokio::test]
    async fn test_status_transition_edge_cases() {
        println!("🔄 Testing status transition edge cases...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let test_op_id = "status-transition-test";

        // Add operation
        let operation = Operation::new(
            test_op_id.to_string(),
            "status transition test".to_string(),
            None,
        );
        monitor.add_operation(operation).await;

        // Try updating status multiple times to Completed
        monitor.update_status(
            test_op_id,
            OperationStatus::Completed,
            Some(Value::String("first completion".to_string())),
        ).await;
        println!("✅ First completion status update");

        // Update status to Completed AGAIN (this might happen in edge cases)
        monitor.update_status(
            test_op_id,
            OperationStatus::Completed,
            Some(Value::String("second completion".to_string())),
        ).await;
        println!("✅ Second completion status update");

        // Check how many completed operations we have
        let completed = monitor.get_completed_operations().await;
        if completed.len() != 1 {
            panic!("STATUS TRANSITION BUG: Expected 1 completed operation, found {}. Multiple status updates may be causing duplicates.", completed.len());
        }

        // Clear operations
        let cleared = monitor.get_and_clear_completed_operations().await;
        assert_eq!(cleared.len(), 1);

        // Try updating status of the already-cleared operation
        monitor.update_status(
            test_op_id,
            OperationStatus::Completed,
            Some(Value::String("post-clear completion".to_string())),
        ).await;
        println!("✅ Post-clear status update (should be ignored)");

        // This should find nothing
        let post_clear_check = monitor.get_and_clear_completed_operations().await;
        if !post_clear_check.is_empty() {
            panic!("POST-CLEAR UPDATE BUG: Found {} operations after post-clear update. Status update after clearing is causing re-appearance!", post_clear_check.len());
        }

        println!("✅ Status transition edge cases test passed");
    }
}
