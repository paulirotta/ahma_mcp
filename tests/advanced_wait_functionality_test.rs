//! Test for advanced await functionality with multiple operations
//!
//! This test verifies that when multiple asynchronous operations are started
//! and then followed by a await call, all operation results are delivered
//! before the await operation completes.

use ahma_mcp::operation_monitor::{MonitorConfig, Operation, OperationMonitor, OperationStatus};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
mod advanced_wait_functionality_test {
    use super::*;

    /// Test that advanced await functionality works correctly with multiple operations
    #[tokio::test]
    async fn test_advanced_wait_with_multiple_operations() {
        println!("🔄 Testing advanced await functionality with multiple operations...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        // Create multiple operations with different tool names
        let op_ids = ["wait_test_op_1", "wait_test_op_2", "wait_test_op_3"];
        let tool_names = ["cargo", "cargo", "npm"]; // Two cargo, one npm

        // Add operations to monitor
        for (i, op_id) in op_ids.iter().enumerate() {
            let operation = Operation::new(
                op_id.to_string(),
                tool_names[i].to_string(),
                format!("Test operation {}", i + 1),
                None,
            );
            monitor.add_operation(operation).await;
            println!("📝 Added operation: {}", op_id);
        }

        // Start the await operation in a background task (simulating async behavior)
        let monitor_clone = monitor.clone();
        let wait_task = tokio::spawn(async move {
            monitor_clone
                .wait_for_operations_advanced(Some("cargo"), Some(30))
                .await
        });

        // Give await operation time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Complete operations one by one (simulating async completion)
        for (i, op_id) in op_ids.iter().enumerate() {
            tokio::time::sleep(Duration::from_millis(200)).await; // Simulate work time

            monitor
                .update_status(
                    op_id,
                    OperationStatus::Completed,
                    Some(Value::String(format!("result_{}", i + 1))),
                )
                .await;
            println!("✅ Completed operation: {}", op_id);
        }

        // Wait for the await operation to complete
        let completed_operations = wait_task.await.unwrap();

        // Verify results
        assert_eq!(
            completed_operations.len(),
            2,
            "Should have 2 cargo operations"
        );

        // Verify that only cargo operations were returned (filter worked)
        for op in &completed_operations {
            assert_eq!(op.tool_name, "cargo", "Should only return cargo operations");
            assert_eq!(op.state, OperationStatus::Completed);
            assert!(op.result.is_some());
        }

        println!(
            "✅ Advanced await test passed - {} cargo operations completed",
            completed_operations.len()
        );
    }

    /// Test timeout warnings and timeout behavior
    #[tokio::test]
    async fn test_advanced_wait_timeout_warnings() {
        println!("⏰ Testing advanced await timeout warnings...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        // Add an operation that we won't complete (to trigger timeout)
        let operation = Operation::new(
            "timeout_test_op".to_string(),
            "test_tool".to_string(),
            "Timeout test operation".to_string(),
            None,
        );
        monitor.add_operation(operation).await;

        // Test with minimum timeout to trigger warnings quickly
        let start_time = std::time::Instant::now();
        let completed_operations = monitor
            .wait_for_operations_advanced(Some("test_tool"), Some(2)) // 2 second timeout
            .await;
        let elapsed = start_time.elapsed();

        // Should timeout and return empty results
        assert!(
            completed_operations.is_empty(),
            "Should timeout with no completed operations"
        );
        assert!(
            elapsed.as_secs() >= 2,
            "Should await at least the timeout duration"
        );
        assert!(
            elapsed.as_secs() < 12,
            "Should not await much longer than timeout"
        );

        println!("✅ Timeout behavior test passed - elapsed: {:?}", elapsed);
    }

    /// Test with no operations (should return immediately)
    #[tokio::test]
    async fn test_advanced_wait_with_no_operations() {
        println!("🔄 Testing advanced await with no operations...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        let start_time = std::time::Instant::now();
        let completed_operations = monitor.wait_for_operations_advanced(None, Some(10)).await;
        let elapsed = start_time.elapsed();

        // Should return immediately with no operations
        assert!(
            completed_operations.is_empty(),
            "Should return empty with no operations"
        );
        assert!(
            elapsed.as_millis() < 500,
            "Should return quickly with no operations"
        );

        println!("✅ No operations test passed - returned immediately");
    }

    /// Test tool filtering functionality
    #[tokio::test]
    async fn test_advanced_wait_tool_filtering() {
        println!("🔍 Testing advanced await tool filtering...");

        let monitor = Arc::new(OperationMonitor::new(MonitorConfig::with_timeout(
            Duration::from_secs(30),
        )));

        // Add operations with different tool names
        let operations = vec![
            ("cargo_op", "cargo"),
            ("npm_op", "npm"),
            ("cargo_other", "cargo"),
            ("python_op", "python"),
        ];

        for (op_id, tool_name) in &operations {
            let operation = Operation::new(
                op_id.to_string(),
                tool_name.to_string(),
                format!("Test {} operation", tool_name),
                None,
            );
            monitor.add_operation(operation).await;

            // Complete the operation immediately
            monitor
                .update_status(
                    op_id,
                    OperationStatus::Completed,
                    Some(Value::String("completed".to_string())),
                )
                .await;
        }

        // Wait for only cargo operations
        let cargo_ops = monitor
            .wait_for_operations_advanced(Some("cargo"), Some(5))
            .await;

        // Wait for npm operations
        let npm_ops = monitor
            .wait_for_operations_advanced(Some("npm"), Some(5))
            .await;

        // Verify filtering worked correctly
        assert_eq!(cargo_ops.len(), 2, "Should find 2 cargo operations");
        assert_eq!(npm_ops.len(), 1, "Should find 1 npm operation");

        for op in &cargo_ops {
            assert!(
                op.tool_name.starts_with("cargo"),
                "Should only return cargo operations"
            );
        }

        for op in &npm_ops {
            assert!(
                op.tool_name.starts_with("npm"),
                "Should only return npm operations"
            );
        }

        println!(
            "✅ Tool filtering test passed - cargo: {}, npm: {}",
            cargo_ops.len(),
            npm_ops.len()
        );
    }
}
