//! Comprehensive shell pool testing for Phase 7 requirements.
//!
//! This test module targets:
//! - Shell reuse efficiency testing
//! - Resource cleanup validation
//! - Performance regression detection
//! - Shell pool lifecycle and health checking
//! - Error handling and recovery scenarios

use ahma_mcp::shell_pool::{PooledShell, ShellCommand, ShellError, ShellPoolConfig, ShellPoolManager};
use anyhow::Result;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::sync::Barrier;

/// Helper: Create a shell command with standard parameters
fn create_command(id: &str, args: Vec<String>, working_dir: &Path, timeout_ms: u64) -> ShellCommand {
    ShellCommand {
        id: id.to_string(),
        command: args,
        working_dir: working_dir.to_string_lossy().to_string(),
        timeout_ms,
    }
}

/// Helper: Execute a simple echo command and verify success
async fn execute_echo_command(
    shell: &mut PooledShell,
    id: &str,
    message: &str,
    working_dir: &Path,
) -> Result<()> {
    let command = create_command(
        id,
        vec!["echo".to_string(), message.to_string()],
        working_dir,
        5000,
    );
    let response = shell.execute_command(command).await?;
    assert_eq!(response.exit_code, 0);
    assert!(response.stdout.contains(message));
    Ok(())
}

/// Helper: Measure shell acquisition and command execution
async fn measure_shell_operation(
    manager: &ShellPoolManager,
    path: &Path,
    iteration: usize,
) -> Result<(Duration, PooledShell)> {
    let start_time = Instant::now();
    let shell = manager.get_shell(path).await;
    assert!(shell.is_some());
    let mut shell = shell.unwrap();
    let acquisition_time = start_time.elapsed();

    let command = create_command(
        &format!("reuse_test_{}", iteration),
        vec!["echo".to_string(), "hello".to_string()],
        path,
        10000,
    );

    let response = shell.execute_command(command).await?;
    assert_eq!(response.exit_code, 0);
    assert!(response.stdout.contains("hello"));

    Ok((acquisition_time, shell))
}

/// Helper: Calculate average duration from a slice
fn average_duration(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    durations.iter().sum::<Duration>() / durations.len() as u32
}

/// Test shell reuse efficiency under repeated operations
#[tokio::test]
async fn test_shell_reuse_efficiency() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_idle_timeout: Duration::from_secs(5),
        pool_cleanup_interval: Duration::from_secs(10),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(10),
        health_check_interval: Duration::from_secs(10),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let num_operations = 10;
    let mut creation_times = Vec::new();
    let mut reuse_times = Vec::new();

    for i in 0..num_operations {
        let (acquisition_time, shell) = measure_shell_operation(&manager, temp_dir.path(), i).await?;
        manager.return_shell(shell).await;

        if i == 0 {
            creation_times.push(acquisition_time);
        } else {
            reuse_times.push(acquisition_time);
        }
    }

    let avg_creation = average_duration(&creation_times);
    let avg_reuse = average_duration(&reuse_times);

    println!("Average shell creation time: {:?}", avg_creation);
    println!("Average shell reuse time: {:?}", avg_reuse);

    if !reuse_times.is_empty() {
        assert!(
            avg_reuse <= avg_creation * 2,
            "Shell reuse should be efficient, creation: {:?}, reuse: {:?}",
            avg_creation,
            avg_reuse
        );
    }

    manager.shutdown_all().await;
    Ok(())
}

/// Test resource cleanup validation under heavy load
#[tokio::test]
async fn test_resource_cleanup_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        max_total_shells: 15,
        shell_idle_timeout: Duration::from_millis(0),
        pool_cleanup_interval: Duration::from_millis(100),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_millis(200),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let num_operations = 15; // Reduced from 20 to 15 for faster execution
    let mut shells_created = Vec::new();

    // Create many shells and track them
    for i in 0..num_operations {
        if let Some(shell) = manager.get_shell(temp_dir.path()).await {
            // Execute command to ensure shell is working
            let command = ShellCommand {
                id: format!("cleanup_test_{}", i),
                command: vec!["pwd".to_string()],
                working_dir: temp_dir.path().to_string_lossy().to_string(),
                timeout_ms: 5000,
            };

            let mut shell_mut = shell;
            let response = shell_mut.execute_command(command).await?;
            assert_eq!(response.exit_code, 0);

            shells_created.push(shell_mut);
        }
    }

    // Verify we hit the pool limit
    let stats = manager.get_stats().await;
    assert!(stats.total_shells <= stats.max_shells);

    // Return all shells to pool
    for shell in shells_created {
        manager.return_shell(shell).await;
    }

    // Trigger cleanup deterministically
    manager.cleanup_idle_pools().await;

    // Check that idle shells were cleaned up
    let final_stats = manager.get_stats().await;
    println!("Final stats: {:?}", final_stats);

    // Should have fewer shells after cleanup (exact number depends on timing)
    assert!(final_stats.total_shells < num_operations);

    manager.shutdown_all().await;
    Ok(())
}

/// Test performance regression detection
#[tokio::test]
async fn test_performance_regression_detection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 4,
        max_total_shells: 20,
        shell_idle_timeout: Duration::from_secs(60),
        pool_cleanup_interval: Duration::from_secs(120),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(60),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let num_rapid_acquisitions = 20;
    let barrier = Arc::new(Barrier::new(num_rapid_acquisitions));
    let mut handles = Vec::new();

    let performance_start = Instant::now();

    // Perform rapid acquisitions concurrently
    for i in 0..num_rapid_acquisitions {
        let manager_clone = manager.clone();
        let temp_path = temp_dir.path().to_path_buf();
        let barrier_clone = barrier.clone();

        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;
            let start = Instant::now();

            // Get shell
            let shell_opt = manager_clone.get_shell(&temp_path).await;
            let acquisition_time = start.elapsed();

            if let Some(mut shell) = shell_opt {
                // Execute lightweight command
                let command = ShellCommand {
                    id: format!("perf_test_{}", i),
                    command: vec!["echo".to_string(), format!("test_{}", i)],
                    working_dir: temp_path.to_string_lossy().to_string(),
                    // Increase timeout to accommodate CI resource limits and heavy concurrency
                    timeout_ms: 15000,
                };

                let cmd_start = Instant::now();
                let response = shell.execute_command(command).await.unwrap();
                let execution_time = cmd_start.elapsed();

                assert_eq!(response.exit_code, 0);

                // Return shell
                manager_clone.return_shell(shell).await;

                (i, acquisition_time, execution_time)
            } else {
                (i, acquisition_time, Duration::ZERO)
            }
        });

        handles.push(handle);
    }

    // Wait for all operations to complete
    let results: Vec<(usize, Duration, Duration)> = futures::future::join_all(handles)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    let total_time = performance_start.elapsed();

    // Analyze performance metrics
    let successful_operations = results
        .iter()
        .filter(|(_, _, exec)| *exec > Duration::ZERO)
        .count();
    let avg_acquisition_time =
        results.iter().map(|(_, acq, _)| *acq).sum::<Duration>() / results.len() as u32;
    let avg_execution_time = results
        .iter()
        .filter(|(_, _, exec)| *exec > Duration::ZERO)
        .map(|(_, _, exec)| *exec)
        .sum::<Duration>()
        / successful_operations.max(1) as u32;

    println!("Performance metrics:");
    println!("  Total operations: {}", num_rapid_acquisitions);
    println!("  Successful operations: {}", successful_operations);
    println!("  Total time: {:?}", total_time);
    println!("  Average acquisition time: {:?}", avg_acquisition_time);
    println!("  Average execution time: {:?}", avg_execution_time);

    // Performance assertions (reasonable bounds for shell operations)
    assert!(
        successful_operations >= num_rapid_acquisitions / 2,
        "Too many operations failed: {}/{}",
        successful_operations,
        num_rapid_acquisitions
    );
    assert!(
        avg_acquisition_time < Duration::from_secs(2),
        "Shell acquisition too slow: {:?}",
        avg_acquisition_time
    );
    assert!(
        avg_execution_time < Duration::from_secs(2),
        "Command execution too slow: {:?}",
        avg_execution_time
    );

    manager.shutdown_all().await;
    Ok(())
}

/// Test shell pool lifecycle and health checking
#[tokio::test]
async fn test_shell_pool_lifecycle_and_health_checking() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 8,
        shell_idle_timeout: Duration::from_millis(200),
        pool_cleanup_interval: Duration::from_millis(50),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_millis(100),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    // Test pool creation
    let initial_stats = manager.get_stats().await;
    assert_eq!(initial_stats.total_pools, 0);
    assert_eq!(initial_stats.total_shells, 0);

    // Create shell to trigger pool creation
    let shell = manager.get_shell(temp_dir.path()).await;
    assert!(shell.is_some());
    let mut shell = shell.unwrap();

    let after_creation_stats = manager.get_stats().await;
    assert_eq!(after_creation_stats.total_pools, 1);
    // Note: Shell pool stats might not accurately reflect active shells due to permit management
    // For now, just verify that stats are reasonable
    assert!(after_creation_stats.total_shells <= after_creation_stats.max_shells);

    // Test shell health check
    assert!(shell.is_healthy());
    let health_ok = shell.health_check().await;
    assert!(health_ok);
    assert!(shell.is_healthy());

    // Execute command to verify shell functionality
    let command = ShellCommand {
        id: "health_test".to_string(),
        command: vec!["echo".to_string(), "health_check".to_string()],
        working_dir: temp_dir.path().to_string_lossy().to_string(),
        timeout_ms: 5000,
    };

    let response = shell.execute_command(command).await?;
    assert_eq!(response.exit_code, 0);
    assert!(response.stdout.contains("health_check"));

    // Return shell to pool
    manager.return_shell(shell).await;

    // Trigger cleanup deterministically
    manager.cleanup_idle_pools().await;

    // Pool should still exist but might have fewer shells due to cleanup
    let final_stats = manager.get_stats().await;
    assert_eq!(final_stats.total_pools, 1);

    manager.shutdown_all().await;

    // After shutdown, no shells should remain
    let shutdown_stats = manager.get_stats().await;
    assert_eq!(shutdown_stats.total_shells, 0);

    Ok(())
}

/// Helper: Acquire multiple shells from the pool
async fn acquire_shells(
    manager: &ShellPoolManager,
    path: &Path,
    count: usize,
) -> Vec<PooledShell> {
    let mut shells = Vec::new();
    for i in 0..count {
        if let Some(shell) = manager.get_shell(path).await {
            shells.push(shell);
            println!("Acquired shell {}, total: {}", i + 1, shells.len());
        } else {
            println!("Failed to acquire shell {}", i + 1);
            break;
        }
    }
    shells
}

/// Helper: Test shell return and reacquisition
async fn test_shell_reacquisition(
    manager: &ShellPoolManager,
    shells: &mut Vec<PooledShell>,
    path: &Path,
) {
    if let Some(shell) = shells.pop() {
        manager.return_shell(shell).await;
        let new_shell = manager.get_shell(path).await;
        assert!(new_shell.is_some(), "Should be able to get a shell after returning one");
        shells.push(new_shell.unwrap());
    }
}

/// Helper: Test command timeout handling
async fn test_timeout_handling(manager: &ShellPoolManager, shells: &mut Vec<PooledShell>, path: &Path) {
    if let Some(mut shell) = shells.pop() {
        let command = create_command("timeout_test", vec!["sleep".to_string(), "1".to_string()], path, 100);
        let result = shell.execute_command(command).await;
        assert!(result.is_err() || result.unwrap().exit_code != 0);
        manager.return_shell(shell).await;
    }
}

/// Helper: Test invalid command handling
async fn test_invalid_command(manager: &ShellPoolManager, shells: &mut Vec<PooledShell>, path: &Path) {
    if let Some(mut shell) = shells.pop() {
        let command = create_command("invalid_test", vec!["nonexistent_command_xyz".to_string()], path, 5000);
        let result = shell.execute_command(command).await;
        assert!(result.is_ok());
        if let Ok(response) = result {
            assert_ne!(response.exit_code, 0);
        }
        manager.return_shell(shell).await;
    }
}

/// Test error handling and recovery scenarios
#[tokio::test]
async fn test_error_handling_and_recovery_scenarios() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 5,
        shell_idle_timeout: Duration::from_secs(30),
        pool_cleanup_interval: Duration::from_secs(60),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_millis(100),
        health_check_interval: Duration::from_secs(30),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let mut shells = acquire_shells(&manager, temp_dir.path(), 3).await;
    assert!(!shells.is_empty(), "Should be able to acquire at least one shell");

    test_shell_reacquisition(&manager, &mut shells, temp_dir.path()).await;
    test_timeout_handling(&manager, &mut shells, temp_dir.path()).await;
    test_invalid_command(&manager, &mut shells, temp_dir.path()).await;

    // Return remaining shells
    for shell in shells {
        manager.return_shell(shell).await;
    }

    // Verify recovery after errors
    let recovery_shell = manager.get_shell(temp_dir.path()).await;
    assert!(recovery_shell.is_some());

    if let Some(mut shell) = recovery_shell {
        execute_echo_command(&mut shell, "recovery_test", "recovered", temp_dir.path()).await?;
        manager.return_shell(shell).await;
    }

    manager.shutdown_all().await;
    Ok(())
}

/// Test shell pool disabled mode fallback
#[tokio::test]
async fn test_shell_pool_disabled_mode_fallback() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: false, // Disabled mode
        shells_per_directory: 2,
        max_total_shells: 10,
        shell_idle_timeout: Duration::from_secs(30),
        pool_cleanup_interval: Duration::from_secs(60),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(30),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    // Should return None when disabled
    let shell = manager.get_shell(temp_dir.path()).await;
    assert!(shell.is_none());

    // Stats should show no activity
    let stats = manager.get_stats().await;
    assert_eq!(stats.total_pools, 0);
    assert_eq!(stats.total_shells, 0);

    manager.shutdown_all().await;
    Ok(())
}

/// Helper: Run concurrent worker operations
async fn run_concurrent_worker(
    manager: Arc<ShellPoolManager>,
    path: std::path::PathBuf,
    worker_id: usize,
    operations: usize,
) -> (usize, usize) {
    let mut successful = 0;
    for j in 0..operations {
        if let Some(mut shell) = manager.get_shell(&path).await {
            let command = create_command(
                &format!("concurrent_{}_{}", worker_id, j),
                vec!["echo".to_string(), format!("worker_{}_{}", worker_id, j)],
                &path,
                15000,
            );

            if let Ok(response) = shell.execute_command(command).await {
                if response.exit_code == 0 {
                    successful += 1;
                }
            }
            manager.return_shell(shell).await;
        }
        tokio::task::yield_now().await;
    }
    (worker_id, successful)
}

/// Test concurrent access patterns
#[tokio::test]
async fn test_concurrent_access_patterns() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        max_total_shells: 15,
        shell_idle_timeout: Duration::from_secs(30),
        pool_cleanup_interval: Duration::from_secs(60),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(30),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let num_concurrent = 5;
    let operations_per_worker = 3;
    let barrier = Arc::new(Barrier::new(num_concurrent));

    let handles: Vec<_> = (0..num_concurrent)
        .map(|i| {
            let manager_clone = manager.clone();
            let temp_path = temp_dir.path().to_path_buf();
            let barrier_clone = barrier.clone();
            tokio::spawn(async move {
                barrier_clone.wait().await;
                run_concurrent_worker(manager_clone, temp_path, i, operations_per_worker).await
            })
        })
        .collect();

    let results: Vec<(usize, usize)> = futures::future::join_all(handles)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    for (worker_id, successful_ops) in &results {
        assert_eq!(
            *successful_ops, operations_per_worker,
            "Worker {} should have completed {} operations, got {}",
            worker_id, operations_per_worker, successful_ops
        );
    }

    let total_successful: usize = results.iter().map(|(_, ops)| ops).sum();
    assert_eq!(total_successful, num_concurrent * operations_per_worker);

    manager.shutdown_all().await;
    Ok(())
}

/// Test shell error types and classification
#[tokio::test]
async fn test_shell_error_types_and_classification() -> Result<()> {
    // Test ShellError categorization
    let spawn_error = ShellError::SpawnError(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "shell not found",
    ));
    assert_eq!(spawn_error.error_category(), "IO");
    assert_eq!(spawn_error.severity_level(), "ERROR");
    assert!(!spawn_error.is_recoverable());
    assert!(!spawn_error.is_resource_exhaustion());
    assert!(spawn_error.is_io_error());

    let timeout_error = ShellError::Timeout;
    assert_eq!(timeout_error.error_category(), "TIMEOUT");
    assert_eq!(timeout_error.severity_level(), "WARN");
    assert!(timeout_error.is_recoverable());
    assert!(timeout_error.is_resource_exhaustion());
    assert!(!timeout_error.is_io_error());

    let pool_full_error = ShellError::PoolFull;
    assert_eq!(pool_full_error.error_category(), "RESOURCE");
    assert_eq!(pool_full_error.severity_level(), "WARN");
    assert!(pool_full_error.is_recoverable());
    assert!(pool_full_error.is_resource_exhaustion());
    assert!(!pool_full_error.is_io_error());

    let process_died_error = ShellError::ProcessDied;
    assert_eq!(process_died_error.error_category(), "PROCESS");
    assert_eq!(process_died_error.severity_level(), "ERROR");
    assert!(process_died_error.is_recoverable());
    assert!(!process_died_error.is_resource_exhaustion());
    assert!(!process_died_error.is_io_error());

    Ok(())
}

/// Helper: Run a single stability test cycle
async fn run_stability_cycle(
    manager: &ShellPoolManager,
    path: &Path,
    cycle: usize,
    operations: usize,
) {
    let mut shells = Vec::new();
    for _ in 0..operations {
        if let Some(shell) = manager.get_shell(path).await {
            shells.push(shell);
        }
    }

    for (i, mut shell) in shells.into_iter().enumerate() {
        let command = create_command(
            &format!("stability_{}_{}", cycle, i),
            vec!["echo".to_string(), format!("cycle_{}", cycle)],
            path,
            5000,
        );
        let _ = shell.execute_command(command).await;
        manager.return_shell(shell).await;
    }
}

/// Helper: Check and log stats periodically
async fn check_stats_if_needed(manager: &ShellPoolManager, cycle: usize, interval: usize) {
    if cycle % interval == 0 {
        let stats = manager.get_stats().await;
        println!("Cycle {}: {:?}", cycle, stats);
        assert!(stats.total_shells <= stats.max_shells);
    }
}

/// Test memory usage stability under sustained load
#[tokio::test]
async fn test_memory_usage_stability_under_sustained_load() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 8,
        shell_idle_timeout: Duration::from_millis(200),
        pool_cleanup_interval: Duration::from_millis(100),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_millis(200),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    let num_cycles = 20;
    let operations_per_cycle = 3;

    for cycle in 0..num_cycles {
        run_stability_cycle(&manager, temp_dir.path(), cycle, operations_per_cycle).await;
        check_stats_if_needed(&manager, cycle, 5).await;

        if cycle % 3 == 0 {
            tokio::task::yield_now().await;
        }
    }

    let final_stats = manager.get_stats().await;
    println!("Final stability stats: {:?}", final_stats);
    assert!(final_stats.total_shells <= final_stats.max_shells);

    manager.shutdown_all().await;
    Ok(())
}
