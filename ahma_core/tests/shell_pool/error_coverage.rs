//! Test coverage for shell pool error handling and edge cases
//!
//! This test module specifically targets untested error paths and edge cases
//! in the shell pool implementation to improve test coverage.

use ahma_core::shell_pool::{PrewarmedShell, ShellError, ShellPoolConfig, ShellPoolManager};
use ahma_core::utils::logging::init_test_logging;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_shell_error_categorization() {
    init_test_logging();

    // Test error recovery checks
    let timeout_error = ShellError::Timeout;
    assert!(timeout_error.is_recoverable());
    assert!(timeout_error.is_resource_exhaustion());
    assert_eq!(timeout_error.error_category(), "TIMEOUT");
    assert_eq!(timeout_error.severity_level(), "WARN");

    let pool_full_error = ShellError::PoolFull;
    assert!(pool_full_error.is_recoverable());
    assert!(pool_full_error.is_resource_exhaustion());
    assert_eq!(pool_full_error.error_category(), "RESOURCE");
    assert_eq!(pool_full_error.severity_level(), "WARN");

    let process_died_error = ShellError::ProcessDied;
    assert!(process_died_error.is_recoverable());
    assert!(!process_died_error.is_resource_exhaustion());
    assert_eq!(process_died_error.error_category(), "PROCESS");
    assert_eq!(process_died_error.severity_level(), "ERROR");

    let io_error = ShellError::SpawnError(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "Permission denied",
    ));
    assert!(!io_error.is_recoverable());
    assert!(!io_error.is_resource_exhaustion());
    assert!(io_error.is_io_error());
    assert_eq!(io_error.error_category(), "IO");
    assert_eq!(io_error.severity_level(), "ERROR");

    let working_dir_error = ShellError::WorkingDirectoryError("Invalid path".to_string());
    assert!(!working_dir_error.is_recoverable());
    assert!(working_dir_error.is_io_error());
    assert_eq!(working_dir_error.error_category(), "IO");
    assert_eq!(working_dir_error.severity_level(), "ERROR");

    let serialization_error = ShellError::SerializationError(
        serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err(),
    );
    assert!(!serialization_error.is_recoverable());
    assert!(!serialization_error.is_resource_exhaustion());
    assert!(!serialization_error.is_io_error());
    assert_eq!(serialization_error.error_category(), "SERIALIZATION");
    assert_eq!(serialization_error.severity_level(), "ERROR");
}

#[tokio::test]
async fn test_shell_pool_manager_at_capacity() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    // Create manager with very low capacity
    let config = ShellPoolConfig {
        enabled: true,
        max_total_shells: 1,
        shells_per_directory: 1,
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    // Get first shell - should succeed
    let shell1 = manager.get_shell(temp_dir.path()).await;
    assert!(shell1.is_some());

    // Try to get second shell - should fail due to capacity
    let shell2 = manager.get_shell(temp_dir.path()).await;
    assert!(shell2.is_none());

    // Return first shell
    if let Some(shell) = shell1 {
        manager.return_shell(shell).await;
    }

    // Now should be able to get a shell again
    let shell3 = manager.get_shell(temp_dir.path()).await;
    assert!(shell3.is_some());

    manager.shutdown_all().await;
}

#[tokio::test]
async fn test_shell_pool_invalid_working_directory() {
    init_test_logging();

    let config = ShellPoolConfig::default();

    // Try to create shell in non-existent directory
    let invalid_path = Path::new("/this/path/definitely/does/not/exist/nowhere");
    let result = PrewarmedShell::new(invalid_path, &config).await;

    // Should fail with spawn error
    assert!(result.is_err());
    if let Err(error) = result {
        assert!(matches!(error, ShellError::SpawnError(_)));
        assert!(error.is_io_error());
    }
}

#[tokio::test]
async fn test_shell_pool_timeout_scenarios() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    let config = ShellPoolConfig {
        enabled: true,
        command_timeout: Duration::from_millis(100), // Very short timeout
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    if let Some(mut shell) = manager.get_shell(temp_dir.path()).await {
        // Create a command that should timeout
        let command = ahma_core::shell_pool::ShellCommand {
            id: "timeout_test".to_string(),
            command: vec!["sleep".to_string(), "5".to_string()], // 5 seconds
            working_dir: temp_dir.path().to_string_lossy().to_string(),
            timeout_ms: 50, // 50ms timeout
        };

        let result = shell.execute_command(command).await;

        // Should timeout
        assert!(result.is_err());
        if let Err(error) = result {
            assert!(matches!(error, ShellError::Timeout));
            assert!(error.is_recoverable());
            assert!(error.is_resource_exhaustion());
        }

        manager.return_shell(shell).await;
    }

    manager.shutdown_all().await;
}

#[tokio::test]
async fn test_shell_pool_health_check_failure_simulation() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    let config = ShellPoolConfig::default();

    if let Ok(mut shell) = PrewarmedShell::new(temp_dir.path(), &config).await {
        // Initially, shell should be healthy
        assert!(shell.is_healthy());
        assert!(shell.health_check().await);

        // Test execution of valid command
        let valid_command = ahma_core::shell_pool::ShellCommand {
            id: "health_test".to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            working_dir: temp_dir.path().to_string_lossy().to_string(),
            timeout_ms: 5000,
        };

        let result = shell.execute_command(valid_command).await;
        assert!(result.is_ok());

        // Shutdown the shell to test cleanup behavior
        shell.shutdown().await;

        // After shutdown, health check should fail
        assert!(!shell.health_check().await);
        assert!(!shell.is_healthy());
    }
}

#[tokio::test]
async fn test_shell_pool_cleanup_and_shutdown() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        shell_idle_timeout: Duration::from_millis(0),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    // Create some shells
    let shell1 = manager.get_shell(temp_dir.path()).await;
    let shell2 = manager.get_shell(temp_dir.path()).await;

    assert!(shell1.is_some());
    assert!(shell2.is_some());

    // Return shells to pool
    if let Some(shell) = shell1 {
        manager.return_shell(shell).await;
    }
    if let Some(shell) = shell2 {
        manager.return_shell(shell).await;
    }

    // Get stats before cleanup
    let stats_before = manager.get_stats().await;
    assert!(stats_before.total_pools > 0);

    // Shells should be immediately eligible for idle cleanup

    // Test cleanup
    manager.cleanup_idle_pools().await;

    // Test shutdown
    manager.shutdown_all().await;

    let stats_after = manager.get_stats().await;
    assert_eq!(stats_after.total_pools, 0);
}

#[tokio::test]
async fn test_shell_pool_return_unhealthy_shell() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    let config = ShellPoolConfig::default();
    let manager = ShellPoolManager::new(config);

    if let Some(mut shell) = manager.get_shell(temp_dir.path()).await {
        // Test returning a shell in different states
        assert!(shell.is_healthy());

        // Shutdown the shell to make it unhealthy
        shell.shutdown().await;

        // Health check should now fail
        assert!(!shell.health_check().await);
        assert!(!shell.is_healthy());

        // Return unhealthy shell - it should be discarded
        manager.return_shell(shell).await;

        // Next shell request should create a new shell
        let new_shell = manager.get_shell(temp_dir.path()).await;
        assert!(new_shell.is_some());

        if let Some(shell) = new_shell {
            manager.return_shell(shell).await;
        }
    }

    manager.shutdown_all().await;
}

#[tokio::test]
async fn test_shell_command_with_invalid_json() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    let config = ShellPoolConfig::default();

    if let Ok(mut shell) = PrewarmedShell::new(temp_dir.path(), &config).await {
        // Create a command with complex data that might cause JSON issues
        let command = ahma_core::shell_pool::ShellCommand {
            id: "json_test".to_string(),
            command: vec![
                "echo".to_string(),
                "'{\"complex\": \"json\\nwith\\\"quotes\"}\'".to_string(),
            ],
            working_dir: temp_dir.path().to_string_lossy().to_string(),
            timeout_ms: 5000,
        };

        let result = shell.execute_command(command).await;

        // Should succeed even with complex JSON in the command
        assert!(result.is_ok());

        shell.shutdown().await;
    }
}

#[tokio::test]
async fn test_shell_drop_behavior() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();

    let config = ShellPoolConfig::default();

    // Create shell and immediately drop it to test Drop implementation
    {
        let shell = PrewarmedShell::new(temp_dir.path(), &config).await;
        assert!(shell.is_ok());
        // Shell drops here, testing the Drop implementation
    }

    // Create another shell to verify the first one was properly cleaned up
    let shell2 = PrewarmedShell::new(temp_dir.path(), &config).await;
    assert!(shell2.is_ok());

    if let Ok(mut shell) = shell2 {
        shell.shutdown().await;
    }
}

#[tokio::test]
async fn test_shell_pool_config_edge_cases() {
    init_test_logging();

    // Test config with zero shells per directory
    let config_zero = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 0,
        max_total_shells: 10,
        ..Default::default()
    };

    let temp_dir = TempDir::new().unwrap();
    let manager = ShellPoolManager::new(config_zero);

    if let Some(shell) = manager.get_shell(temp_dir.path()).await {
        // Should still work but returning shell will always discard it
        manager.return_shell(shell).await;
    }

    manager.shutdown_all().await;

    // Test config with very small timeouts
    let config_small_timeout = ShellPoolConfig {
        enabled: true,
        shell_spawn_timeout: Duration::from_millis(1),
        command_timeout: Duration::from_millis(1),
        shell_idle_timeout: Duration::from_millis(1),
        ..Default::default()
    };

    let manager2 = ShellPoolManager::new(config_small_timeout);

    // This might fail due to tiny timeouts, but should handle gracefully
    let shell = manager2.get_shell(temp_dir.path()).await;
    if let Some(shell) = shell {
        manager2.return_shell(shell).await;
    }

    manager2.shutdown_all().await;
}
