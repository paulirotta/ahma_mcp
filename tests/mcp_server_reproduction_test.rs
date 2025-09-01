#[cfg(test)]
mod mcp_server_reproduction_test {
    use ahma_mcp::operation_monitor::{
        MonitorConfig, Operation, OperationMonitor, OperationStatus,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    /// This test reproduces exactly what the MCP server does:
    /// 1. Add operations
    /// 2. Complete them
    /// 3. Run the notification loop (get_and_clear_completed_operations every 1 second)
    /// 4. Verify that operations are cleared and don't repeat
    #[tokio::test]
    async fn test_mcp_server_notification_loop_reproduction() {
        println!("🔄 Reproducing MCP server notification loop behavior...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        // Simulate two operations like in the real server log
        let op1_id = "op_test_1";
        let op2_id = "op_test_2";

        // Add first operation (cargo test - will fail)
        let op1 = Operation::new(op1_id.to_string(), "cargo".to_string(), "cargo test".to_string(), None);
        operation_monitor.add_operation(op1).await;

        // Add second operation (cargo check - will succeed)
        let op2 = Operation::new(op2_id.to_string(), "cargo".to_string(), "cargo check".to_string(), None);
        operation_monitor.add_operation(op2).await;

        println!("✅ Added 2 operations to monitor");

        // Complete both operations (like what happens in real execution)
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

        println!("✅ Completed both operations");

        // Now simulate the exact MCP server notification loop
        for iteration in 1..=5 {
            println!("\n--- Notification Loop Iteration {} ---", iteration);

            let completed_ops = operation_monitor.get_and_clear_completed_operations().await;

            println!("Found {} completed operations", completed_ops.len());

            for op in &completed_ops {
                println!("  - Operation {}: {:?}", op.id, op.state);
            }

            // THE CRITICAL TEST: After the first iteration, no operations should be found
            if iteration == 1 {
                if completed_ops.len() != 2 {
                    panic!(
                        "FIRST ITERATION BUG: Expected 2 operations in first iteration, found {}",
                        completed_ops.len()
                    );
                }
                println!("✅ First iteration found expected 2 operations");
            } else {
                if !completed_ops.is_empty() {
                    panic!(
                        "ENDLESS LOOP BUG REPRODUCED: Iteration {} found {} operations - they should have been cleared!",
                        iteration,
                        completed_ops.len()
                    );
                }
                println!(
                    "✅ Iteration {} found no operations (correctly cleared)",
                    iteration
                );
            }

            // Wait 1 second like the real server does
            sleep(Duration::from_secs(1)).await;
        }

        println!("\n✅ MCP server notification loop reproduction test PASSED");
    }
}
