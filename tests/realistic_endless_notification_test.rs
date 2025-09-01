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

    /// This test reproduces the EXACT scenario from the user's logs:
    /// 1. Run an async operation (like cargo version)
    /// 2. Simulate the main.rs notification loop running every 1 second
    /// 3. Check if the same operation keeps being notified endlessly
    #[tokio::test]
    async fn test_realistic_endless_notification_scenario() {
        println!("🔬 Testing realistic endless notification scenario...");

        // Set up the exact same components as main.rs
        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        let shell_pool_config = ShellPoolConfig {
            enabled: true,
            shells_per_directory: 2,
            max_total_shells: 20,
            shell_idle_timeout: Duration::from_secs(1800),
            pool_cleanup_interval: Duration::from_secs(300),
            shell_spawn_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
        };
        let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
        shell_pool_manager.clone().start_background_tasks();

        let adapter =
            Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager.clone()).unwrap());

        let configs = Arc::new(load_tool_configs(&std::path::PathBuf::from("tools")).unwrap());
        let _service = AhmaMcpService::new(adapter.clone(), operation_monitor.clone(), configs)
            .await
            .unwrap();

        println!("✅ Set up realistic environment");

        // Execute the same type of operation that shows in user's logs
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

        // Wait for operation to complete
        let mut completion_wait_attempts = 0;
        loop {
            sleep(Duration::from_millis(200)).await;
            let completed_ops = operation_monitor.get_completed_operations().await;
            if !completed_ops.is_empty() {
                println!(
                    "✅ Operation completed after {} attempts",
                    completion_wait_attempts + 1
                );
                break;
            }
            completion_wait_attempts += 1;
            if completion_wait_attempts > 50 {
                // 10 second timeout
                panic!("Operation never completed");
            }
        }

        // Now simulate the EXACT notification loop from main.rs
        println!("🔄 Simulating notification loop behavior...");

        let mut notification_history = Vec::new();

        // Run the notification loop 6 times with 1-second delays (like main.rs)
        for iteration in 1..=6 {
            // This is the EXACT code from main.rs line 165
            let completed_ops = operation_monitor.get_and_clear_completed_operations().await;

            let found_operations: Vec<String> =
                completed_ops.iter().map(|op| op.id.clone()).collect();

            notification_history.push((iteration, found_operations.clone()));

            println!(
                "📊 Iteration {}: Found {} completed operations: {:?}",
                iteration,
                found_operations.len(),
                found_operations
            );

            // The BUG: if the same operation appears in multiple iterations,
            // that's the endless notification loop!
            if iteration > 1 && !found_operations.is_empty() {
                // Check if any operation from this iteration was seen before
                let mut bug_detected = false;
                for op_id in &found_operations {
                    for (prev_iteration, prev_operations) in
                        &notification_history[..notification_history.len() - 1]
                    {
                        if prev_operations.contains(op_id) {
                            println!("🐛 ENDLESS NOTIFICATION BUG DETECTED!");
                            println!(
                                "   Operation {} appeared in iteration {} and again in iteration {}",
                                op_id, prev_iteration, iteration
                            );
                            bug_detected = true;
                        }
                    }
                }

                if bug_detected {
                    println!("📋 Full notification history:");
                    for (iter, ops) in &notification_history {
                        println!("   Iteration {}: {:?}", iter, ops);
                    }
                    panic!(
                        "ENDLESS NOTIFICATION LOOP BUG REPRODUCED! This matches the user's log pattern."
                    );
                }
            }

            // Wait exactly 1 second like the real notification loop
            sleep(Duration::from_secs(1)).await;
        }

        println!("✅ Realistic test completed - no endless notifications detected");
        println!("📋 Final notification history:");
        for (iter, ops) in &notification_history {
            println!(
                "   Iteration {}: {} operations - {:?}",
                iter,
                ops.len(),
                ops
            );
        }

        // The first iteration should find the operation, subsequent ones should be empty
        assert!(
            !notification_history[0].1.is_empty(),
            "First iteration should find completed operation"
        );
        for (iteration, operations) in &notification_history[1..] {
            assert!(
                operations.is_empty(),
                "Iteration {} should find no operations but found: {:?}",
                iteration,
                operations
            );
        }
    }

    /// Test if multiple operations can cause notification chaos
    #[tokio::test]
    async fn test_multiple_operations_notification_behavior() {
        println!("🔢 Testing multiple operations notification behavior...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        let shell_pool_config = ShellPoolConfig {
            enabled: true,
            shells_per_directory: 2,
            max_total_shells: 20,
            shell_idle_timeout: Duration::from_secs(1800),
            pool_cleanup_interval: Duration::from_secs(300),
            shell_spawn_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
        };
        let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
        shell_pool_manager.clone().start_background_tasks();

        let adapter =
            Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager).unwrap());

        // Start multiple operations quickly
        let op1 = adapter
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

        let op2 = adapter
            .execute_async_in_dir(
                "cargo",
                "--version",
                Some(serde_json::Map::from_iter(vec![(
                    "_subcommand".to_string(),
                    serde_json::Value::String("--version".to_string()),
                )])),
                "/Users/paul/github/ahma_mcp",
                Some(30),
            )
            .await;

        println!("🚀 Started operations: {} and {}", op1, op2);

        // Wait for both to complete
        let mut completion_attempts = 0;
        loop {
            sleep(Duration::from_millis(300)).await;
            let completed_ops = operation_monitor.get_completed_operations().await;
            if completed_ops.len() >= 2 {
                println!("✅ Both operations completed");
                break;
            }
            completion_attempts += 1;
            if completion_attempts > 33 {
                // 10 second timeout
                let completed_ops = operation_monitor.get_completed_operations().await;
                if completed_ops.len() == 1 {
                    println!("⚠️  Only 1 operation completed, proceeding with test");
                    break;
                } else {
                    panic!(
                        "Operations never completed: {} completed",
                        completed_ops.len()
                    );
                }
            }
        }

        // Test notification behavior with multiple operations
        let mut all_seen_operations = std::collections::HashSet::new();

        for iteration in 1..=4 {
            let completed_ops = operation_monitor.get_and_clear_completed_operations().await;
            let current_ops: Vec<String> = completed_ops.iter().map(|op| op.id.clone()).collect();

            println!(
                "📊 Iteration {}: Found {} operations: {:?}",
                iteration,
                current_ops.len(),
                current_ops
            );

            // Check for duplicates across iterations
            for op_id in &current_ops {
                if all_seen_operations.contains(op_id) {
                    panic!(
                        "ENDLESS NOTIFICATION BUG: Operation {} seen again in iteration {}",
                        op_id, iteration
                    );
                }
                all_seen_operations.insert(op_id.clone());
            }

            sleep(Duration::from_millis(500)).await;
        }

        println!("✅ Multiple operations test passed - no endless notifications");
    }
}
