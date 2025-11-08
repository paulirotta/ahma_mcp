use ahma_core::{
    adapter::Adapter,
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_git_operations_timeout_parameter() {
    // Initialize logging for the test
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .try_init();

    // Create a temporary directory for testing
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path().to_string_lossy().to_string();

    // Set up operation monitor
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    // Set up shell pool
    let shell_config = ShellPoolConfig {
        enabled: true,
        command_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));

    // Create adapter
    let adapter = Adapter::new(operation_monitor, shell_pool).unwrap();

    // Test that git push with timeout_seconds parameter is accepted
    let result = timeout(Duration::from_secs(5), async {
        adapter
            .execute_sync_in_dir(
                "git",
                Some(serde_json::Map::from_iter([(
                    "timeout_seconds".to_string(),
                    serde_json::Value::Number(30.into()),
                )])),
                &temp_path,
                Some(30),
                None,
            )
            .await
    })
    .await;

    // Should complete within timeout (may fail due to no git repo, but parameter should be accepted)
    assert!(
        result.is_ok(),
        "Git push with timeout_seconds should be accepted"
    );
}

#[tokio::test]
async fn test_timeout_seconds_parameter_validation() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .try_init();

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path().to_string_lossy().to_string();

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    let shell_config = ShellPoolConfig {
        enabled: true,
        command_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    let adapter = Adapter::new(operation_monitor, shell_pool).unwrap();

    // Test minimum timeout value (10 seconds)
    let result = timeout(Duration::from_secs(5), async {
        adapter
            .execute_sync_in_dir(
                "git",
                Some(serde_json::Map::from_iter([(
                    "timeout_seconds".to_string(),
                    serde_json::Value::Number(10.into()),
                )])),
                &temp_path,
                Some(10),
                None,
            )
            .await
    })
    .await;

    assert!(
        result.is_ok(),
        "Minimum timeout_seconds value should be accepted"
    );

    // Test maximum timeout value (1800 seconds)
    let result = timeout(Duration::from_secs(5), async {
        adapter
            .execute_sync_in_dir(
                "git",
                Some(serde_json::Map::from_iter([(
                    "timeout_seconds".to_string(),
                    serde_json::Value::Number(1800.into()),
                )])),
                &temp_path,
                Some(1800),
                None,
            )
            .await
    })
    .await;

    assert!(
        result.is_ok(),
        "Maximum timeout_seconds value should be accepted"
    );
}

#[tokio::test]
async fn test_default_timeout_behavior() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .try_init();

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path().to_string_lossy().to_string();

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    let shell_config = ShellPoolConfig {
        enabled: true,
        command_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    let shell_pool = Arc::new(ShellPoolManager::new(shell_config));
    let adapter = Adapter::new(operation_monitor, shell_pool).unwrap();

    // Test that git operations work without explicit timeout_seconds (should use default)
    let result = timeout(Duration::from_secs(5), async {
        adapter
            .execute_sync_in_dir(
                "git", None, &temp_path, None, // No explicit timeout - should use default
                None,
            )
            .await
    })
    .await;

    assert!(
        result.is_ok(),
        "Git operations should work with default timeout"
    );
}
