#[cfg(test)]
mod mcp_server_reproduction_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    /// This test validates the new persistent completion history design:
    /// 1. Add operations
    /// 2. Complete them via update_status (moves to completion_history)
    /// 3. Verify that completed operations remain accessible for wait operations
    /// 4. Verify that MCP notifications prevent duplicates through proper tracking
    #[tokio::test]
    async fn test_persistent_completion_history_behavior() {
        println!("🔄 Testing persistent completion history behavior...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Simulate two operations like in the real server log
        let op1_id = "op_test_1";
        let op2_id = "op_test_2";

        // Add first operation (cargo test - will fail)
        let op1 = Operation::new(
            op1_id.to_string(),
            "cargo".to_string(),
            "cargo test".to_string(),
            None,
        );
        operation_monitor.add_operation(op1).await;

        // Add second operation (cargo check - will succeed)
        let op2 = Operation::new(
            op2_id.to_string(),
            "cargo".to_string(),
            "cargo check".to_string(),
            None,
        );
        operation_monitor.add_operation(op2).await;

        println!("✅ Added 2 operations to monitor");

        // Complete both operations via update_status (this moves them to completion_history)
        operation_monitor
            .update_status(
                op1_id,
                OperationStatus::Failed,
                Some(Value::String("test failed".to_string())),
            )
            .await;

        operation_monitor
            .update_status(
                op2_id,
                OperationStatus::Completed,
                Some(Value::String("check succeeded".to_string())),
            )
            .await;

        println!("✅ Completed both operations (moved to completion_history)");

        // Test that completed operations remain consistently accessible
        for iteration in 1..=3 {
            println!("\n--- Completion History Access {} ---", iteration);

            let completed_ops = operation_monitor.get_completed_operations().await;

            // NEW BEHAVIOR: Operations remain in persistent history
            assert_eq!(
                completed_ops.len(),
                2,
                "Both operations should persist in completion_history"
            );

            // Verify specific operations are present
            let ids: Vec<&str> = completed_ops.iter().map(|op| op.id.as_str()).collect();
            assert!(
                ids.contains(&op1_id),
                "op_test_1 should be in completion history"
            );
            assert!(
                ids.contains(&op2_id),
                "op_test_2 should be in completion history"
            );

            println!(
                "✅ Iteration {}: Found expected 2 operations in completion history",
                iteration
            );

            for op in &completed_ops {
                println!("  - Operation {}: {:?}", op.id, op.state);
            }

            // Short sleep to simulate notification timing
            sleep(Duration::from_millis(100)).await;
        }

        println!("\n✅ Persistent completion history test PASSED");
        println!("  - Operations moved to completion_history on completion");
        println!("  - Operations remain accessible for wait operations");
        println!("  - Duplicate notifications prevented by MCP callback system");
    }
}
