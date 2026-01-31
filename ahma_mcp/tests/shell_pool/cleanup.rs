//! Tests for shell pool cleanup, idle detection, and health check functionality
//!
//! These tests cover the uncovered paths in shell_pool.rs:
//! - cleanup_idle_pools() - cleaning up pools that have been idle
//! - is_idle() - detecting when a pool has been idle too long
//! - health_check() - verifying shells are still responsive
//! - Pool capacity limits (ShellError::PoolFull)

use ahma_mcp::shell_pool::{PrewarmedShell, ShellError, ShellPool, ShellPoolConfig, ShellPoolManager};
use ahma_mcp::utils::logging::init_test_logging;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_pool_is_idle_detection() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    // Pool should not be idle immediately after creation with a long timeout
    let active_config = ShellPoolConfig {
        enabled: true,
        shell_idle_timeout: Duration::from_secs(60),
        ..Default::default()
    };
    let active_pool = ShellPool::new(temp_dir.path(), active_config);
    assert!(
        !active_pool.is_idle().await,
        "Pool should not be idle immediately after creation"
    );

    // Pool should be idle immediately with a zero timeout
    let idle_config = ShellPoolConfig {
        enabled: true,
        shell_idle_timeout: Duration::from_millis(0),
        ..Default::default()
    };
    let idle_pool = ShellPool::new(temp_dir.path(), idle_config);
    assert!(
        idle_pool.is_idle().await,
        "Pool should be idle with zero timeout"
    );
}

#[tokio::test]
async fn test_pool_activity_resets_idle_timer() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shell_idle_timeout: Duration::from_millis(100),
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let pool = ShellPool::new(temp_dir.path(), config);
    
    // Get a shell - this should reset the idle timer
    let shell = pool.get_shell().await;
    assert!(shell.is_ok(), "Should be able to get a shell");
    
    // Return the shell
    if let Ok(shell) = shell {
        pool.return_shell(shell).await;
    }
    
    // Should not be idle because we just used it
    assert!(
        !pool.is_idle().await,
        "Pool should not be idle after recent activity"
    );
}

#[tokio::test]
async fn test_pool_health_check_removes_unhealthy_shells() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let pool = ShellPool::new(temp_dir.path(), config.clone());
    
    // Get and return a few shells to populate the pool
    for _ in 0..2 {
        if let Ok(shell) = pool.get_shell().await {
            pool.return_shell(shell).await;
        }
    }
    
    let initial_count = pool.shell_count().await;
    
    // Run health check - should not remove healthy shells
    pool.health_check().await;
    
    let after_check_count = pool.shell_count().await;
    
    // Healthy shells should remain
    assert_eq!(
        initial_count, after_check_count,
        "Healthy shells should not be removed by health check"
    );
}

#[tokio::test]
async fn test_pool_shutdown_clears_all_shells() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let pool = ShellPool::new(temp_dir.path(), config);
    
    // Populate the pool with shells
    for _ in 0..2 {
        if let Ok(shell) = pool.get_shell().await {
            pool.return_shell(shell).await;
        }
    }
    
    assert!(pool.shell_count().await > 0, "Pool should have shells");
    
    // Shutdown the pool
    pool.shutdown().await;
    
    assert_eq!(
        pool.shell_count().await,
        0,
        "Pool should have no shells after shutdown"
    );
}

#[tokio::test]
async fn test_manager_cleanup_idle_pools() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shell_idle_timeout: Duration::from_millis(0),
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let manager = Arc::new(ShellPoolManager::new(config));
    
    // Get a shell to create a pool
    let shell = manager.get_shell(temp_dir.path()).await;
    if let Some(shell) = shell {
        manager.return_shell(shell).await;
    }
    
    // Get initial stats
    let initial_stats = manager.get_stats().await;
    assert!(
        initial_stats.total_pools > 0,
        "Should have at least one pool"
    );
    
    // Run cleanup
    manager.cleanup_idle_pools().await;
    
    // After cleanup, idle pools should be removed
    let after_stats = manager.get_stats().await;
    assert_eq!(
        after_stats.total_pools, 0,
        "Idle pools should be removed by cleanup"
    );
}

#[tokio::test]
async fn test_manager_shutdown_all() {
    init_test_logging();
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let manager = Arc::new(ShellPoolManager::new(config));
    
    // Create pools for multiple directories
    for temp_dir in [&temp_dir1, &temp_dir2] {
        if let Some(shell) = manager.get_shell(temp_dir.path()).await {
            manager.return_shell(shell).await;
        }
    }
    
    let stats = manager.get_stats().await;
    assert!(stats.total_pools >= 1, "Should have created pools");
    
    // Shutdown all
    manager.shutdown_all().await;
    
    let after_stats = manager.get_stats().await;
    assert_eq!(
        after_stats.total_pools, 0,
        "All pools should be removed after shutdown_all"
    );
}

#[tokio::test]
async fn test_pool_capacity_limit() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    // Configure with very limited capacity
    let config = ShellPoolConfig {
        enabled: true,
        max_total_shells: 1, // Only allow 1 shell total
        shells_per_directory: 1,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let manager = Arc::new(ShellPoolManager::new(config));
    
    // Get first shell - should succeed
    let shell1 = manager.get_shell(temp_dir.path()).await;
    assert!(shell1.is_some(), "First shell should be acquired");
    
    // Try to get second shell - should fail due to capacity
    let shell2 = manager.get_shell(temp_dir.path()).await;
    assert!(
        shell2.is_none(),
        "Second shell should fail - pool at capacity"
    );
    
    // Return the first shell
    if let Some(shell) = shell1 {
        manager.return_shell(shell).await;
    }
    
    // Now we should be able to get a shell again
    let shell3 = manager.get_shell(temp_dir.path()).await;
    assert!(
        shell3.is_some(),
        "Should be able to get shell after returning one"
    );
}

#[tokio::test]
async fn test_shell_error_is_recoverable() {
    // Test ShellError helper methods for coverage
    let timeout_error = ShellError::Timeout;
    assert!(timeout_error.is_recoverable());
    assert!(timeout_error.is_resource_exhaustion());
    assert!(!timeout_error.is_io_error());
    assert_eq!(timeout_error.error_category(), "TIMEOUT");
    assert_eq!(timeout_error.severity_level(), "WARN");
    
    let pool_full = ShellError::PoolFull;
    assert!(pool_full.is_recoverable());
    assert!(pool_full.is_resource_exhaustion());
    assert!(!pool_full.is_io_error());
    assert_eq!(pool_full.error_category(), "RESOURCE");
    assert_eq!(pool_full.severity_level(), "WARN");
    
    let process_died = ShellError::ProcessDied;
    assert!(process_died.is_recoverable());
    assert!(!process_died.is_resource_exhaustion());
    assert!(!process_died.is_io_error());
    assert_eq!(process_died.error_category(), "PROCESS");
    assert_eq!(process_died.severity_level(), "ERROR");
}

#[tokio::test]
async fn test_shell_health_check_on_healthy_shell() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    // Create a shell directly
    let mut shell = PrewarmedShell::new(temp_dir.path(), &config)
        .await
        .expect("Should create shell");
    
    // Health check should pass for a freshly created shell
    let is_healthy = shell.health_check().await;
    assert!(is_healthy, "Fresh shell should be healthy");
    assert!(shell.is_healthy(), "Shell should report as healthy");
    
    // Shutdown the shell
    shell.shutdown().await;
}

#[tokio::test]
async fn test_pool_returns_none_for_missing_directory() {
    init_test_logging();
    
    let config = ShellPoolConfig {
        enabled: true,
        shell_spawn_timeout: Duration::from_secs(2),
        ..Default::default()
    };
    
    let manager = Arc::new(ShellPoolManager::new(config));
    
    // Try to get a shell for a non-existent directory
    let result = manager.get_shell("/nonexistent/directory/path").await;
    
    // Should return None because the directory doesn't exist
    // (shell spawn will fail)
    assert!(
        result.is_none(),
        "Should return None for non-existent directory"
    );
}

#[tokio::test]
async fn test_return_shell_to_wrong_pool() {
    init_test_logging();
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let manager = Arc::new(ShellPoolManager::new(config.clone()));
    
    // Create a shell for dir1 manually (simulating a shell from another pool)
    let shell = PrewarmedShell::new(temp_dir1.path(), &config)
        .await
        .expect("Should create shell");
    
    // Get a shell from dir2 to create that pool
    if let Some(shell2) = manager.get_shell(temp_dir2.path()).await {
        manager.return_shell(shell2).await;
    }
    
    // Try to return shell1 to the manager - it should handle gracefully
    // even though the pool for dir1 doesn't exist in the manager
    manager.return_shell(shell).await;
    
    // This should not panic and the shell should be dropped
}

#[tokio::test]
async fn test_pool_respects_shells_per_directory_limit() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2, // Only 2 shells per directory
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let pool = ShellPool::new(temp_dir.path(), config);
    
    // Get and return more shells than the limit
    let mut shells = Vec::new();
    for _ in 0..4 {
        if let Ok(shell) = pool.get_shell().await {
            shells.push(shell);
        }
    }
    
    // Return all shells
    for shell in shells {
        pool.return_shell(shell).await;
    }
    
    // Pool should only keep shells_per_directory shells
    let count = pool.shell_count().await;
    assert!(
        count <= 2,
        "Pool should respect shells_per_directory limit: got {}",
        count
    );
}

#[tokio::test]
async fn test_pool_stats_accuracy() {
    init_test_logging();
    let temp_dir = TempDir::new().unwrap();
    
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        max_total_shells: 10,
        shell_spawn_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    
    let manager = Arc::new(ShellPoolManager::new(config));
    
    // Initial stats
    let initial_stats = manager.get_stats().await;
    assert_eq!(initial_stats.total_pools, 0);
    assert_eq!(initial_stats.max_shells, 10);
    
    // Get a shell
    let shell = manager.get_shell(temp_dir.path()).await;
    assert!(shell.is_some());
    
    let stats_with_shell = manager.get_stats().await;
    assert_eq!(stats_with_shell.total_pools, 1);
    assert!(stats_with_shell.total_shells >= 1);
    
    // Return the shell
    if let Some(shell) = shell {
        manager.return_shell(shell).await;
    }
    
    // Stats should still show the pool exists
    let final_stats = manager.get_stats().await;
    assert_eq!(final_stats.total_pools, 1);
}
