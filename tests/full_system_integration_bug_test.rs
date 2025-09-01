#[cfg(test)]
mod full_system_integration_bug_test {
    use ahma_mcp::adapter::Adapter;
    use ahma_mcp::callback_system::{CallbackSender, ProgressUpdate};
    use ahma_mcp::config::load_tool_configs;
    use ahma_mcp::mcp_service::AhmaMcpService;
    use ahma_mcp::operation_monitor::{MonitorConfig, OperationMonitor};
    use ahma_mcp::shell_pool::{ShellPoolConfig, ShellPoolManager};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;

    /// A mock callback that tracks all notifications sent
    #[derive(Debug, Clone)]
    struct TrackingCallback {
        notifications: Arc<Mutex<Vec<ProgressUpdate>>>,
    }

    impl TrackingCallback {
        fn new() -> Self {
            Self {
                notifications: Arc::new(Mutex::new(Vec::new())),
            }
        }

        #[allow(dead_code)]
        fn get_notifications(&self) -> Vec<ProgressUpdate> {
            self.notifications.lock().unwrap().clone()
        }

        fn count_by_operation_id(&self, operation_id: &str) -> usize {
            self.notifications
                .lock()
                .unwrap()
                .iter()
                .filter(|update| update.operation_id() == operation_id)
                .count()
        }

        fn count_completed_notifications(&self, operation_id: &str) -> usize {
            self.notifications
                .lock()
                .unwrap()
                .iter()
                .filter(|update| {
                    update.operation_id() == operation_id
                        && matches!(update, ProgressUpdate::Completed { .. })
                })
                .count()
        }
    }

    #[async_trait::async_trait]
    impl CallbackSender for TrackingCallback {
        async fn send_progress(
            &self,
            update: ProgressUpdate,
        ) -> Result<(), ahma_mcp::callback_system::CallbackError> {
            println!("📨 TRACKED NOTIFICATION: {}", update);
            self.notifications.lock().unwrap().push(update);
            Ok(())
        }

        async fn should_cancel(&self) -> bool {
            false
        }
    }

    /// Test the full system integration to see if endless notifications emerge
    /// This test simulates the exact conditions that might exist in production
    #[tokio::test]
    async fn test_full_system_integration_endless_notifications() {
        println!("🔍 Testing full system integration for endless notifications...");

        // Set up the complete system
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

        let configs = Arc::new(load_tool_configs(&std::path::PathBuf::from("tools")).unwrap());
        let _service = AhmaMcpService::new(adapter.clone(), operation_monitor.clone(), configs)
            .await
            .unwrap();

        // Create tracking callback to monitor all notifications
        let tracking_callback = Arc::new(TrackingCallback::new());

        println!("✅ Full system initialized");

        // Start a cargo operation
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

        println!("🚀 Started operation: {}", operation_id);

        // Wait for operation to complete
        let mut completion_wait = 0;
        loop {
            sleep(Duration::from_millis(200)).await;
            let completed_ops = operation_monitor.get_completed_operations().await;
            if !completed_ops.is_empty() {
                println!("✅ Operation completed");
                break;
            }
            completion_wait += 1;
            if completion_wait > 50 {
                // 10 second timeout
                panic!("Operation never completed");
            }
        }

        // Now simulate the main.rs notification loop behavior
        // This is the CRITICAL part where the endless loop bug might occur
        println!("🔄 Simulating main.rs notification loop...");

        let mut notification_counts_per_iteration = Vec::new();

        for iteration in 1..=10 {
            // Simulate the exact main.rs notification loop code
            let completed_ops = operation_monitor.get_and_clear_completed_operations().await;

            if !completed_ops.is_empty() {
                println!(
                    "📊 Iteration {}: Found {} completed operations",
                    iteration,
                    completed_ops.len()
                );

                // Send notifications for each completed operation (like main.rs does)
                for op in &completed_ops {
                    let progress_update = match (&op.state, &op.result) {
                        (ahma_mcp::operation_monitor::OperationStatus::Completed, Some(result)) => {
                            ProgressUpdate::Completed {
                                operation_id: op.id.clone(),
                                message: format!("Operation completed successfully: {}", result),
                                duration_ms: 1000,
                            }
                        }
                        _ => ProgressUpdate::Completed {
                            operation_id: op.id.clone(),
                            message: "Operation finished".to_string(),
                            duration_ms: 1000,
                        },
                    };

                    tracking_callback
                        .send_progress(progress_update)
                        .await
                        .unwrap();
                }

                notification_counts_per_iteration.push((iteration, completed_ops.len()));
            } else {
                println!("📊 Iteration {}: Found 0 completed operations", iteration);
                notification_counts_per_iteration.push((iteration, 0));
            }

            sleep(Duration::from_secs(1)).await; // 1-second delay like main.rs
        }

        // Analyze the results for endless notification patterns
        println!("📈 Notification analysis:");
        for (iteration, count) in &notification_counts_per_iteration {
            println!("   Iteration {}: {} notifications", iteration, count);
        }

        // Check for the endless notification bug pattern
        let total_notifications = tracking_callback.count_by_operation_id(&operation_id);
        let completed_notifications =
            tracking_callback.count_completed_notifications(&operation_id);

        println!("🎯 Final analysis for operation {}:", operation_id);
        println!("   Total notifications: {}", total_notifications);
        println!("   Completed notifications: {}", completed_notifications);

        // The bug: if the same operation appears in multiple iterations
        let iterations_with_notifications: Vec<usize> = notification_counts_per_iteration
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(iteration, _)| *iteration)
            .collect();

        if iterations_with_notifications.len() > 1 {
            println!("🐛 ENDLESS NOTIFICATION BUG DETECTED!");
            println!(
                "   Operation appeared in iterations: {:?}",
                iterations_with_notifications
            );
            println!(
                "   This matches the user's log pattern of repeated notifications every second!"
            );

            panic!(
                "ENDLESS NOTIFICATION BUG REPRODUCED: Operation {} was notified {} times across {} iterations. Expected: 1 notification total.",
                operation_id,
                completed_notifications,
                iterations_with_notifications.len()
            );
        }

        // The operation should be notified exactly once
        if completed_notifications != 1 {
            panic!(
                "NOTIFICATION COUNT BUG: Expected 1 completed notification for operation {}, but got {}",
                operation_id, completed_notifications
            );
        }

        if iterations_with_notifications.len() != 1 {
            panic!(
                "NOTIFICATION SPREAD BUG: Expected operation to appear in 1 iteration, but appeared in {}",
                iterations_with_notifications.len()
            );
        }

        println!("✅ Full system integration test passed - no endless notifications detected");
        println!(
            "   Operation {} was properly notified exactly once in iteration {}",
            operation_id, iterations_with_notifications[0]
        );
    }

    /// Test what happens when the system is under load with multiple operations
    #[tokio::test]
    async fn test_multiple_operations_system_integration() {
        println!("⚡ Testing multiple operations system integration...");

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

        let shell_pool_config = ShellPoolConfig {
            enabled: true,
            shells_per_directory: 3,
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

        // Wait for all operations to complete
        let mut completion_wait = 0;
        loop {
            sleep(Duration::from_millis(300)).await;
            let completed_ops = operation_monitor.get_completed_operations().await;
            if completed_ops.len() >= 2 {
                println!("✅ All operations completed");
                break;
            }
            completion_wait += 1;
            if completion_wait > 33 {
                let completed_ops = operation_monitor.get_completed_operations().await;
                if completed_ops.len() >= 1 {
                    println!("⚠️  At least 1 operation completed, proceeding");
                    break;
                } else {
                    panic!("No operations completed in time");
                }
            }
        }

        // Track all operations found across iterations
        let mut all_operations_seen: std::collections::HashMap<String, Vec<usize>> = HashMap::new();

        // Simulate notification loop with multiple operations
        for iteration in 1..=6 {
            let completed_ops = operation_monitor.get_and_clear_completed_operations().await;

            println!(
                "📊 Iteration {}: Found {} operations",
                iteration,
                completed_ops.len()
            );

            for op in &completed_ops {
                all_operations_seen
                    .entry(op.id.clone())
                    .or_insert_with(Vec::new)
                    .push(iteration);
                println!("   - Operation {}: {}", op.id, op.state.is_terminal());
            }

            sleep(Duration::from_millis(500)).await;
        }

        // Check for endless notification patterns
        println!("🔍 Analysis of operation appearances:");
        for (op_id, iterations) in &all_operations_seen {
            println!(
                "   Operation {}: appeared in iterations {:?}",
                op_id, iterations
            );

            if iterations.len() > 1 {
                panic!(
                    "MULTIPLE OPERATIONS ENDLESS NOTIFICATION BUG: Operation {} appeared {} times in iterations {:?}",
                    op_id,
                    iterations.len(),
                    iterations
                );
            }
        }

        println!("✅ Multiple operations integration test passed - no endless notifications");
    }
}
