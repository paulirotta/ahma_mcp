#[cfg(test)]
mod async_notification_test {
    use ahma_mcp::adapter::Adapter;
    use ahma_mcp::config::load_tool_configs;
    use ahma_mcp::mcp_service::AhmaMcpService;
    use ahma_mcp::operation_monitor::{MonitorConfig, OperationMonitor};
    use ahma_mcp::shell_pool::{ShellPoolConfig, ShellPoolManager};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_operation_completion_tracking() {
        println!("Testing if operations are properly tracked to completion...");

        // Create the components
        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let operation_monitor = Arc::new(OperationMonitor::new(monitor_config.clone()));

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
        let adapter =
            Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager).unwrap());

        // Load configs and create service (not used in this test)
        let configs = Arc::new(load_tool_configs(&std::path::PathBuf::from("tools")).unwrap());
        let _service = AhmaMcpService::new(
            adapter.clone(),
            operation_monitor.clone(),
            configs,
            Arc::new(None),
        )
        .await
        .unwrap();

        println!("Starting a quick async operation...");

        // Test using direct adapter call to see what happens with the fix
        let job_id = adapter
            .execute_async_in_dir(
                "cargo",   // Use base command
                "version", // command
                Some(serde_json::Map::from_iter(vec![(
                    "_subcommand".to_string(),
                    serde_json::Value::String("version".to_string()),
                )])),
                "/Users/paul/github/ahma_mcp",
                Some(10),
            )
            .await;

        println!("Started operation with job ID: {}", job_id);

        // Wait for completion
        let completed_op = operation_monitor.wait_for_operation(&job_id).await;
        assert!(completed_op.is_some(), "Operation did not complete in time");

        let completed_ops = operation_monitor.get_completed_operations().await;
        println!("Found {} completed operations", completed_ops.len());

        if !completed_ops.is_empty() {
            for op in &completed_ops {
                println!("  Operation {} - Status: {:?}", op.id, op.state);
                if let Some(result) = &op.result {
                    println!("  Result: {}", result);
                }
            }
        }
    }
}
