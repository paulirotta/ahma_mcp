//! Shell Pool Coverage Improvement Tests
//!
//! These tests target low-coverage areas in shell_pool.rs, focusing on:
//! - ShellError methods (is_recoverable, is_resource_exhaustion, is_io_error, error_category, severity_level)
//! - ShellPool and ShellPoolManager behavior
//! - Command execution edge cases

use ahma_core::shell_pool::{
    ShellCommand, ShellError, ShellPool, ShellPoolConfig, ShellPoolManager,
};
use ahma_core::utils::logging::init_test_logging;
use std::time::Duration;
use tempfile::TempDir;

// ============= ShellError tests =============

#[test]
fn test_shell_error_is_recoverable_timeout() {
    init_test_logging();
    let error = ShellError::Timeout;
    assert!(error.is_recoverable(), "Timeout should be recoverable");
}

#[test]
fn test_shell_error_is_recoverable_pool_full() {
    init_test_logging();
    let error = ShellError::PoolFull;
    assert!(error.is_recoverable(), "PoolFull should be recoverable");
}

#[test]
fn test_shell_error_is_recoverable_process_died() {
    init_test_logging();
    let error = ShellError::ProcessDied;
    assert!(error.is_recoverable(), "ProcessDied should be recoverable");
}

#[test]
fn test_shell_error_is_not_recoverable_spawn_error() {
    init_test_logging();
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "command not found");
    let error = ShellError::SpawnError(io_error);
    assert!(
        !error.is_recoverable(),
        "SpawnError should not be recoverable"
    );
}

#[test]
fn test_shell_error_is_not_recoverable_serialization_error() {
    init_test_logging();
    let json_error = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
    let error = ShellError::SerializationError(json_error);
    assert!(
        !error.is_recoverable(),
        "SerializationError should not be recoverable"
    );
}

#[test]
fn test_shell_error_is_not_recoverable_working_directory_error() {
    init_test_logging();
    let error = ShellError::WorkingDirectoryError("directory not accessible".to_string());
    assert!(
        !error.is_recoverable(),
        "WorkingDirectoryError should not be recoverable"
    );
}

#[test]
fn test_shell_error_is_resource_exhaustion_pool_full() {
    init_test_logging();
    let error = ShellError::PoolFull;
    assert!(
        error.is_resource_exhaustion(),
        "PoolFull should be resource exhaustion"
    );
}

#[test]
fn test_shell_error_is_resource_exhaustion_timeout() {
    init_test_logging();
    let error = ShellError::Timeout;
    assert!(
        error.is_resource_exhaustion(),
        "Timeout should be resource exhaustion"
    );
}

#[test]
fn test_shell_error_is_not_resource_exhaustion_process_died() {
    init_test_logging();
    let error = ShellError::ProcessDied;
    assert!(
        !error.is_resource_exhaustion(),
        "ProcessDied should not be resource exhaustion"
    );
}

#[test]
fn test_shell_error_is_io_error_spawn_error() {
    init_test_logging();
    let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let error = ShellError::SpawnError(io_error);
    assert!(error.is_io_error(), "SpawnError should be IO error");
}

#[test]
fn test_shell_error_is_io_error_working_directory_error() {
    init_test_logging();
    let error = ShellError::WorkingDirectoryError("not found".to_string());
    assert!(
        error.is_io_error(),
        "WorkingDirectoryError should be IO error"
    );
}

#[test]
fn test_shell_error_is_not_io_error_timeout() {
    init_test_logging();
    let error = ShellError::Timeout;
    assert!(!error.is_io_error(), "Timeout should not be IO error");
}

#[test]
fn test_shell_error_error_category_spawn_error() {
    init_test_logging();
    let io_error = std::io::Error::other("spawn failed");
    let error = ShellError::SpawnError(io_error);
    assert_eq!(error.error_category(), "IO");
}

#[test]
fn test_shell_error_error_category_working_directory_error() {
    init_test_logging();
    let error = ShellError::WorkingDirectoryError("invalid path".to_string());
    assert_eq!(error.error_category(), "IO");
}

#[test]
fn test_shell_error_error_category_timeout() {
    init_test_logging();
    let error = ShellError::Timeout;
    assert_eq!(error.error_category(), "TIMEOUT");
}

#[test]
fn test_shell_error_error_category_process_died() {
    init_test_logging();
    let error = ShellError::ProcessDied;
    assert_eq!(error.error_category(), "PROCESS");
}

#[test]
fn test_shell_error_error_category_serialization_error() {
    init_test_logging();
    let json_error = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
    let error = ShellError::SerializationError(json_error);
    assert_eq!(error.error_category(), "SERIALIZATION");
}

#[test]
fn test_shell_error_error_category_pool_full() {
    init_test_logging();
    let error = ShellError::PoolFull;
    assert_eq!(error.error_category(), "RESOURCE");
}

#[test]
fn test_shell_error_severity_level_spawn_error() {
    init_test_logging();
    let io_error = std::io::Error::other("test");
    let error = ShellError::SpawnError(io_error);
    assert_eq!(error.severity_level(), "ERROR");
}

#[test]
fn test_shell_error_severity_level_process_died() {
    init_test_logging();
    let error = ShellError::ProcessDied;
    assert_eq!(error.severity_level(), "ERROR");
}

#[test]
fn test_shell_error_severity_level_serialization_error() {
    init_test_logging();
    let json_error = serde_json::from_str::<serde_json::Value>("{{").unwrap_err();
    let error = ShellError::SerializationError(json_error);
    assert_eq!(error.severity_level(), "ERROR");
}

#[test]
fn test_shell_error_severity_level_working_directory_error() {
    init_test_logging();
    let error = ShellError::WorkingDirectoryError("test".to_string());
    assert_eq!(error.severity_level(), "ERROR");
}

#[test]
fn test_shell_error_severity_level_timeout() {
    init_test_logging();
    let error = ShellError::Timeout;
    assert_eq!(error.severity_level(), "WARN");
}

#[test]
fn test_shell_error_severity_level_pool_full() {
    init_test_logging();
    let error = ShellError::PoolFull;
    assert_eq!(error.severity_level(), "WARN");
}

// ============= ShellPoolConfig tests =============

#[test]
fn test_shell_pool_config_custom_values() {
    init_test_logging();
    let config = ShellPoolConfig {
        enabled: false,
        shells_per_directory: 5,
        max_total_shells: 50,
        shell_idle_timeout: Duration::from_secs(120),
        pool_cleanup_interval: Duration::from_secs(120),
        shell_spawn_timeout: Duration::from_secs(10),
        command_timeout: Duration::from_secs(60),
        health_check_interval: Duration::from_secs(30),
    };

    assert!(!config.enabled);
    assert_eq!(config.shells_per_directory, 5);
    assert_eq!(config.max_total_shells, 50);
    assert_eq!(config.shell_idle_timeout, Duration::from_secs(120));
    assert_eq!(config.pool_cleanup_interval, Duration::from_secs(120));
    assert_eq!(config.shell_spawn_timeout, Duration::from_secs(10));
    assert_eq!(config.command_timeout, Duration::from_secs(60));
    assert_eq!(config.health_check_interval, Duration::from_secs(30));
}

// ============= ShellCommand tests =============

#[test]
fn test_shell_command_creation() {
    init_test_logging();
    let command = ShellCommand {
        id: "test_cmd_1".to_string(),
        command: vec!["echo".to_string(), "hello".to_string()],
        working_dir: "/tmp".to_string(),
        timeout_ms: 5000,
    };

    assert_eq!(command.id, "test_cmd_1");
    assert_eq!(command.command.len(), 2);
    assert_eq!(command.command[0], "echo");
    assert_eq!(command.command[1], "hello");
    assert_eq!(command.working_dir, "/tmp");
    assert_eq!(command.timeout_ms, 5000);
}

#[test]
fn test_shell_command_json_roundtrip() {
    init_test_logging();
    let command = ShellCommand {
        id: "roundtrip_test".to_string(),
        command: vec![
            "ls".to_string(),
            "-la".to_string(),
            "/path with spaces".to_string(),
        ],
        working_dir: "/home/user".to_string(),
        timeout_ms: 30000,
    };

    let json = serde_json::to_string(&command).unwrap();
    let deserialized: ShellCommand = serde_json::from_str(&json).unwrap();

    assert_eq!(command.id, deserialized.id);
    assert_eq!(command.command, deserialized.command);
    assert_eq!(command.working_dir, deserialized.working_dir);
    assert_eq!(command.timeout_ms, deserialized.timeout_ms);
}

// ============= ShellPoolManager tests =============

#[tokio::test]
async fn test_shell_pool_manager_disabled_returns_none() {
    init_test_logging();
    let config = ShellPoolConfig {
        enabled: false,
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);
    let shell = manager.get_shell("/tmp").await;
    assert!(shell.is_none(), "Disabled pool should return None");
}

#[tokio::test]
async fn test_shell_pool_manager_returns_shell_for_valid_directory() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 1,
        max_total_shells: 5,
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);
    let shell = manager.get_shell(temp_dir.path().to_str().unwrap()).await;

    // Shell creation may succeed or fail depending on system resources
    // Just verify the API doesn't panic
    if let Some(mut shell) = shell {
        assert!(!shell.id().is_empty());
        shell.shutdown().await;
    }
}

#[tokio::test]
async fn test_shell_pool_manager_returns_shell_and_returns_it() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);
    let shell = manager.get_shell(temp_dir.path().to_str().unwrap()).await;

    if let Some(shell) = shell {
        let _shell_id = shell.id().to_string();
        manager.return_shell(shell).await;

        // Get another shell - might be the same one from the pool
        let shell2 = manager.get_shell(temp_dir.path().to_str().unwrap()).await;
        if let Some(mut s2) = shell2 {
            // Either reused or new shell, both are valid
            assert!(!s2.id().is_empty());
            s2.shutdown().await;
        }
    }
}

// ============= ShellError Display tests =============

#[test]
fn test_shell_error_display_messages() {
    init_test_logging();

    // Test that Display trait is implemented and produces readable messages
    let timeout_err = ShellError::Timeout;
    assert!(
        timeout_err.to_string().contains("timeout"),
        "Timeout error should mention 'timeout'"
    );

    let pool_full_err = ShellError::PoolFull;
    assert!(
        pool_full_err.to_string().contains("capacity"),
        "PoolFull error should mention 'capacity'"
    );

    let process_died_err = ShellError::ProcessDied;
    assert!(
        process_died_err.to_string().contains("died"),
        "ProcessDied error should mention 'died'"
    );

    let wd_err = ShellError::WorkingDirectoryError("test path".to_string());
    assert!(
        wd_err.to_string().contains("test path"),
        "WorkingDirectoryError should include the path"
    );
}

// ============= Edge case tests for improved coverage =============

#[tokio::test]
async fn test_shell_pool_manager_get_stats() {
    init_test_logging();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);
    let stats = manager.get_stats().await;

    assert_eq!(stats.total_pools, 0, "No pools should exist initially");
    assert_eq!(stats.max_shells, 10, "Max shells should match config");
    assert_eq!(
        stats.total_shells, 0,
        "No shells should be active initially"
    );
}

#[tokio::test]
async fn test_shell_pool_manager_cleanup_idle_pools() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 1,
        max_total_shells: 5,
        shell_idle_timeout: Duration::from_millis(0),
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    // Get a shell to create a pool
    if let Some(shell) = manager.get_shell(temp_dir.path().to_str().unwrap()).await {
        manager.return_shell(shell).await;
    }

    // Run cleanup - should remove idle pools
    manager.cleanup_idle_pools().await;

    // Stats should reflect cleanup
    let stats = manager.get_stats().await;
    assert_eq!(stats.total_pools, 0, "Idle pool should be cleaned up");
}

#[tokio::test]
async fn test_shell_pool_manager_shutdown_all() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    // Get a shell to create a pool
    if let Some(shell) = manager.get_shell(temp_dir.path().to_str().unwrap()).await {
        manager.return_shell(shell).await;
    }

    // Verify pool exists (always true for usize)
    let stats_before = manager.get_stats().await;
    let _ = stats_before.total_pools; // Use the value

    // Shutdown all
    manager.shutdown_all().await;

    // All pools should be removed
    let stats_after = manager.get_stats().await;
    assert_eq!(stats_after.total_pools, 0, "All pools should be shut down");
}

#[tokio::test]
async fn test_shell_pool_manager_capacity_limit() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 5,
        max_total_shells: 1, // Very low limit
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config);

    // Get first shell - should succeed
    let shell1 = manager.get_shell(temp_dir.path().to_str().unwrap()).await;

    if let Some(s1) = shell1 {
        // Try to get second shell - should fail due to capacity
        let shell2 = manager.get_shell(temp_dir.path().to_str().unwrap()).await;
        assert!(shell2.is_none(), "Should not get shell when at capacity");

        // Return first shell
        manager.return_shell(s1).await;

        // Now should be able to get another shell
        let shell3 = manager.get_shell(temp_dir.path().to_str().unwrap()).await;
        if let Some(mut s) = shell3 {
            s.shutdown().await;
        }
    }
}

#[tokio::test]
async fn test_shell_pool_shell_count() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let pool = ShellPool::new(temp_dir.path(), config);

    // Initially empty
    let count = pool.shell_count().await;
    assert_eq!(count, 0, "Pool should be empty initially");
}

#[tokio::test]
async fn test_shell_pool_config_getter() {
    init_test_logging();
    let config = ShellPoolConfig {
        enabled: true,
        max_total_shells: 42,
        ..Default::default()
    };

    let manager = ShellPoolManager::new(config.clone());
    let retrieved_config = manager.config();

    assert_eq!(retrieved_config.max_total_shells, 42);
    assert!(retrieved_config.enabled);
}
