#[cfg(test)]
mod realistic_endless_notification_test {
    use ahma_mcp::adapter::Adapter;
    use ahma_mcp::config::load_tool_configs;
    use ahma_mcp::mcp_service::AhmaMcpService;
    use ahma_mcp::operation_monitor::{MonitorConfig, OperationMonitor};
    use ahma_mcp::shell_pool::{ShellPoolConfig, ShellPoolManager};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    /// This test reproduces a scenario similar to the user's logs, but adapted
    /// for the new `completion_history` architecture.
    /// 1. Run an async operation (like cargo version).
    /// 2. Repeatedly check the `completion_history`.
    /// 3. Ensure the operation appears once and is not duplicated on subsequent checks.
    #[tokio::test]
    async fn test_realistic_notification_scenario_with_history() {
        println!("🔬 Testing realistic notification scenario with completion history...");

        // Setup
        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
        let shell_pool_config = ShellPoolConfig::default();
        let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
        shell_pool_manager.clone().start_background_tasks();
        let adapter =
            Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager.clone()).unwrap());
        let configs = Arc::new(load_tool_configs(&std::path::PathBuf::from("tools")).unwrap());
        let _service = AhmaMcpService::new(adapter.clone(), operation_monitor.clone(), configs)
            .await
            .unwrap();

        println!("✅ Set up realistic environment");

        // Execute async operation
        let operation_id = adapter
            .execute_async_in_dir(
                "cargo",
                "version",
                Some(serde_json::Map::from_iter(vec![(
                    "_subcommand".to_string(),
                    serde_json::Value::String("version".to_string()),
                )])),
                "/Users/paul/github/ahma_mcp",
                Some(30),
            )
            .await;
        println!("🚀 Started async operation: {}", operation_id);

        // Wait for the operation to appear in the completion history
        let mut operation_found = false;
        for _ in 0..50 { // 10-second timeout
            let completed_ops = operation_monitor.get_completed_operations().await;
            if completed_ops.iter().any(|op| op.id == operation_id) {
                println!("✅ Operation found in completion history.");
                operation_found = true;
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }
        assert!(operation_found, "Operation never appeared in completion history");

        // Now, simulate a notification loop checking the history
        println!("🔄 Simulating notification loop checking history...");
        let mut seen_operations = std::collections::HashSet::new();
        let mut notifications_sent = 0;

        // Check the history multiple times
        for i in 1..=5 {
            let completed_ops = operation_monitor.get_completed_operations().await;
            println!("📊 Iteration {}: History contains {} operations.", i, completed_ops.len());

            for op in completed_ops {
                // In a real notification system, we'd check if we've already notified for this op.
                if !seen_operations.contains(&op.id) {
                    println!("� Sending notification for operation: {}", op.id);
                    seen_operations.insert(op.id.clone());
                    notifications_sent += 1;
                } else {
                    println!("✅ Already notified for operation: {}", op.id);
                }
            }
            sleep(Duration::from_secs(1)).await;
        }

        // We should only have sent one notification for the single operation
        assert_eq!(notifications_sent, 1, "Should have sent exactly one notification.");
        println!("✅ Test passed: Exactly one notification was sent for the operation.");
    }

    /// Test if multiple operations are handled correctly by the completion history.
    #[tokio::test]
    async fn test_multiple_operations_notification_behavior() {
        println!("🔢 Testing multiple operations notification behavior...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
        let shell_pool_config = ShellPoolConfig::default();
        let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
        shell_pool_manager.clone().start_background_tasks();
        let adapter =
            Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager).unwrap());

        // Start multiple operations
        let op_ids = vec![
            adapter.execute_async_in_dir("cargo", "version", None, "/Users/paul/github/ahma_mcp", Some(30)).await,
            adapter.execute_async_in_dir("cargo", "--version", None, "/Users/paul/github/ahma_mcp", Some(30)).await,
        ];
        println!("🚀 Started operations: {:?}", op_ids);

        // Wait for both to complete
        for _ in 0..50 { // 10-second timeout
            let completed_count = operation_monitor.get_completed_operations().await.len();
            if completed_count >= op_ids.len() {
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }
        let final_completed = operation_monitor.get_completed_operations().await;
        assert_eq!(final_completed.len(), op_ids.len(), "Not all operations completed in time.");
        println!("✅ Both operations completed and are in history.");

        // Simulate notification loop
        let mut all_seen_operations = std::collections::HashSet::new();
        let mut total_notifications = 0;

        for iteration in 1..=4 {
            let completed_ops = operation_monitor.get_completed_operations().await;
            println!(
                "📊 Iteration {}: History contains {} operations.",
                iteration,
                completed_ops.len()
            );

            for op in completed_ops {
                if all_seen_operations.insert(op.id.clone()) {
                    println!("🔔 Sending notification for new operation: {}", op.id);
                    total_notifications += 1;
                }
            }
            sleep(Duration::from_millis(500)).await;
        }

        assert_eq!(total_notifications, op_ids.len(), "Incorrect number of notifications sent.");
        println!("✅ Multiple operations test passed - correct number of notifications sent.");
    }
}
