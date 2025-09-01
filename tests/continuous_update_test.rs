#[cfg(test)]
mod continuous_update_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;

    /// This test simulates a scenario where update_status is called repeatedly
    /// after operations have been cleared - could this cause re-addition?
    #[tokio::test]
    async fn test_continuous_updates_after_clear() {
        println!("🔄 Testing continuous status updates after operations are cleared...");
        
        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
        
        let op_id = "continuous-update-test";
        
        // Add and complete operation
        let operation = Operation::new(
            op_id.to_string(),
            "continuous update test".to_string(),
            None,
        );
        operation_monitor.add_operation(operation).await;
        operation_monitor.update_status(
            op_id,
            OperationStatus::Completed,
            Some(Value::String("initial completion".to_string())),
        ).await;
        
        println!("✅ Added and completed operation");
        
        // Clear the operation (like notification loop does)
        let first_clear = operation_monitor.get_and_clear_completed_operations().await;
        assert_eq!(first_clear.len(), 1);
        println!("✅ Cleared operation (found {} operations)", first_clear.len());
        
        // Now continuously try to update the status (simulate potential race conditions)
        for i in 1..=5 {
            operation_monitor.update_status(
                op_id,
                OperationStatus::Completed,
                Some(Value::String(format!("update attempt {}", i))),
            ).await;
            
            println!("📝 Status update attempt {}", i);
            
            // Check if this caused the operation to reappear
            let check_clear = operation_monitor.get_and_clear_completed_operations().await;
            if !check_clear.is_empty() {
                panic!("BUG: Status update {} caused {} operations to reappear!", i, check_clear.len());
            }
        }
        
        println!("✅ No operations reappeared after continuous status updates");
        
        // Final verification
        let final_clear = operation_monitor.get_and_clear_completed_operations().await;
        assert!(final_clear.is_empty(), "Final check should find no operations");
        
        println!("✅ Test passed - continuous updates don't cause re-addition");
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
                format!("continuous add test {}", i),
                None,
            );
            operation_monitor.add_operation(operation).await;
            println!("📝 Add operation attempt {}", i);
        }
        
        // Complete the operation
        operation_monitor.update_status(
            op_id,
            OperationStatus::Completed,
            Some(Value::String("completed".to_string())),
        ).await;
        
        // Check how many completed operations there are 
        let completed = operation_monitor.get_completed_operations().await;
        println!("📊 Found {} completed operations", completed.len());
        
        // Should be exactly 1, not 3
        if completed.len() != 1 {
            panic!("MULTIPLE ADD BUG: Expected 1 completed operation, found {}. Multiple add_operation calls are causing duplicates.", completed.len());
        }
        
        // Clear and verify only one operation is cleared
        let cleared = operation_monitor.get_and_clear_completed_operations().await;
        assert_eq!(cleared.len(), 1);
        
        // Subsequent clears should be empty
        let second_clear = operation_monitor.get_and_clear_completed_operations().await;
        assert!(second_clear.is_empty());
        
        println!("✅ Test passed - multiple add_operation calls don't cause duplicates");
    }
}
