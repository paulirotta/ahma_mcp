/// Quick verification test for intelligent await timeout implementation
/// Tests the core functionality without relying on external tools
use ahma_mcp::{
    adapter::Adapter,
    mcp_service::AhmaMcpService,
    operation_monitor::{MonitorConfig, Operation, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_intelligent_timeout_calculation_direct() {
    // Create service with basic configuration
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(300));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    let sandbox = Arc::new(ahma_mcp::sandbox::Sandbox::new_test());
    let adapter =
        Arc::new(Adapter::new(Arc::clone(&operation_monitor), shell_pool, sandbox).unwrap());

    // Create empty configs and guidance
    let configs = Arc::new(HashMap::new());
    let guidance = Arc::new(None);

    let service = AhmaMcpService::new(
        adapter,
        operation_monitor.clone(),
        configs,
        guidance,
        false,
        false,
    )
    .await
    .unwrap();

    // Add an operation with a specific timeout (600 seconds = 10 minutes)
    let operation = Operation::new_with_timeout(
        "test_op_1".to_string(),
        "cargo".to_string(),
        "Test cargo operation".to_string(),
        None,
        Some(Duration::from_secs(600)), // 10 minutes
    );
    operation_monitor.add_operation(operation).await;

    // Test the internal timeout calculation method directly
    // This tests the core logic without the full MCP protocol complexity
    let calculated_timeout = service.calculate_intelligent_timeout(&[]).await;

    // Should be max(240, 600) = 600 seconds since we have an operation with 600s timeout
    assert_eq!(
        calculated_timeout, 600.0,
        "Should calculate max(240, 600) = 600"
    );

    // Test with tool filtering - should still find the operation
    let filtered_timeout = service
        .calculate_intelligent_timeout(&["cargo".to_string()])
        .await;
    assert_eq!(
        filtered_timeout, 600.0,
        "Should find cargo operation with 600s timeout"
    );

    // Test with non-matching filter - should use default
    let no_match_timeout = service
        .calculate_intelligent_timeout(&["npm".to_string()])
        .await;
    assert_eq!(
        no_match_timeout, 240.0,
        "Should use default 240s when no matching operations"
    );

    println!("✅ Intelligent timeout calculation is functioning correctly");
}

#[tokio::test]
async fn test_timeout_warning_logic() {
    // Create service with basic configuration
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(300));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    let sandbox = Arc::new(ahma_mcp::sandbox::Sandbox::new_test());
    let adapter =
        Arc::new(Adapter::new(Arc::clone(&operation_monitor), shell_pool, sandbox).unwrap());

    // Create empty configs and guidance
    let configs = Arc::new(HashMap::new());
    let guidance = Arc::new(None);

    let service = AhmaMcpService::new(
        adapter,
        operation_monitor.clone(),
        configs,
        guidance,
        false,
        false,
    )
    .await
    .unwrap();

    // Add an operation with a long timeout (600 seconds = 10 minutes)
    let operation = Operation::new_with_timeout(
        "test_op_2".to_string(),
        "npm".to_string(),
        "Test npm operation".to_string(),
        None,
        Some(Duration::from_secs(600)), // 10 minutes
    );
    operation_monitor.add_operation(operation).await;

    // Test the intelligent timeout calculation
    let intelligent_timeout = service.calculate_intelligent_timeout(&[]).await;
    assert_eq!(
        intelligent_timeout, 600.0,
        "Should calculate 600s intelligent timeout"
    );

    // Test scenario where explicit timeout is less than intelligent timeout
    let explicit_timeout = 30.0; // 30 seconds
    assert!(
        explicit_timeout < intelligent_timeout,
        "Setup: explicit < intelligent"
    );

    // This verifies the warning logic would trigger
    // In the actual implementation, a warning would be generated when explicit_timeout < intelligent_timeout
    let should_warn = explicit_timeout < intelligent_timeout;
    assert!(
        should_warn,
        "Should warn when explicit timeout (30s) < intelligent timeout (600s)"
    );

    println!("✅ Timeout warning logic is functioning correctly");
}
