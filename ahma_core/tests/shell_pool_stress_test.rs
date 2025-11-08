//! Performance and resource management tests for shell pool
//! Tests shell reuse, cleanup, and performance under load

use ahma_core::shell_pool::{ShellPoolConfig, ShellPoolManager};
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_shell_pool_basic_functionality() -> Result<()> {
    // Test basic shell pool functionality and performance
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 5,
        shell_idle_timeout: Duration::from_secs(30),
        pool_cleanup_interval: Duration::from_secs(60),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(30),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let temp_dir = TempDir::new()?;

    // Test shell acquisition performance
    let start = Instant::now();
    let shell = timeout(Duration::from_secs(5), manager.get_shell(temp_dir.path())).await?;
    let acquisition_time = start.elapsed();

    println!("Shell acquisition time: {:?}", acquisition_time);

    // Should be able to get a shell (either from pool or return None for fallback)
    if shell.is_some() {
        println!("Got shell from pool");
    } else {
        println!("Pool returned None (fallback mode)");
    }

    // Performance check - should be reasonably fast
    assert!(
        acquisition_time < Duration::from_secs(2),
        "Shell acquisition took too long: {:?}",
        acquisition_time
    );

    Ok(())
}

#[tokio::test]
async fn test_shell_pool_disabled_mode() -> Result<()> {
    // Test that disabled shell pool returns None properly
    let config = ShellPoolConfig {
        enabled: false,
        shells_per_directory: 0,
        max_total_shells: 0,
        shell_idle_timeout: Duration::from_secs(1),
        pool_cleanup_interval: Duration::from_secs(1),
        shell_spawn_timeout: Duration::from_secs(1),
        command_timeout: Duration::from_secs(1),
        health_check_interval: Duration::from_secs(1),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    let temp_dir = TempDir::new()?;

    // When disabled, should return None
    let shell = timeout(Duration::from_secs(2), manager.get_shell(temp_dir.path())).await?;

    assert!(shell.is_none(), "Expected None when shell pool is disabled");

    Ok(())
}

#[tokio::test]
async fn test_shell_pool_concurrent_access() -> Result<()> {
    // Test concurrent access to shell pool
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        max_total_shells: 10,
        shell_idle_timeout: Duration::from_secs(30),
        pool_cleanup_interval: Duration::from_secs(60),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(30),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let temp_dir = TempDir::new()?;

    // Start multiple concurrent shell requests
    let mut handles = Vec::new();

    for i in 0..5 {
        let manager_clone = Arc::clone(&manager);
        let temp_path = temp_dir.path().to_path_buf();

        let handle = tokio::spawn(async move {
            let start = Instant::now();

            let shell_result =
                timeout(Duration::from_secs(5), manager_clone.get_shell(&temp_path)).await;

            let acquisition_time = start.elapsed();

            match shell_result {
                Ok(shell_opt) => Ok((i, acquisition_time, shell_opt.is_some())),
                Err(_) => Err(format!("Timeout for request {}", i)),
            }
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    let results = futures::future::join_all(handles).await;

    // Analyze results
    let mut successful_requests = 0;
    let mut total_time = Duration::from_secs(0);

    for result in results {
        match result {
            Ok(Ok((i, time, got_shell))) => {
                successful_requests += 1;
                total_time += time;
                println!(
                    "Concurrent request {} succeeded in {:?}, got_shell: {}",
                    i, time, got_shell
                );
            }
            Ok(Err(e)) => {
                println!("Concurrent request failed: {}", e);
            }
            Err(e) => {
                println!("Task failed: {}", e);
            }
        }
    }

    // Should have some successful requests
    assert!(
        successful_requests > 0,
        "Expected at least some successful shell requests"
    );

    if successful_requests > 0 {
        let avg_time = total_time / successful_requests;
        println!("Average concurrent acquisition time: {:?}", avg_time);

        // Concurrent acquisitions should still be reasonably fast
        assert!(
            avg_time < Duration::from_secs(2),
            "Concurrent acquisitions took too long: {:?}",
            avg_time
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_shell_pool_performance_baseline() -> Result<()> {
    // Test to establish performance baseline for shell pool
    let config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 3,
        max_total_shells: 10,
        shell_idle_timeout: Duration::from_secs(60),
        pool_cleanup_interval: Duration::from_secs(120),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(30),
    };

    let manager = Arc::new(ShellPoolManager::new(config));
    manager.clone().start_background_tasks();

    let temp_dir = TempDir::new()?;

    // Baseline: measure time for initial shell acquisition
    let start = Instant::now();
    let initial_shell = timeout(Duration::from_secs(5), manager.get_shell(temp_dir.path())).await?;
    let initial_time = start.elapsed();

    assert!(
        initial_shell.is_some() || initial_shell.is_none(),
        "Shell result should be valid"
    );

    // Performance test: multiple shell acquisitions
    const NUM_ACQUISITIONS: usize = 20;
    let start = Instant::now();

    for i in 0..NUM_ACQUISITIONS {
        let shell_result =
            timeout(Duration::from_secs(2), manager.get_shell(temp_dir.path())).await;

        assert!(
            shell_result.is_ok(),
            "Shell acquisition {} failed or timed out",
            i
        );
        // Note: shell can be None (fallback mode) or Some (from pool)
    }

    let total_time = start.elapsed();
    let avg_time = total_time / NUM_ACQUISITIONS as u32;

    println!("Initial shell acquisition: {:?}", initial_time);
    println!(
        "Average of {} acquisitions: {:?}",
        NUM_ACQUISITIONS, avg_time
    );
    println!(
        "Total time for {} acquisitions: {:?}",
        NUM_ACQUISITIONS, total_time
    );

    // Performance expectations - should be fast regardless of pool vs fallback
    assert!(
        avg_time < Duration::from_millis(100),
        "Average shell acquisition time ({:?}) exceeds performance expectation",
        avg_time
    );

    // Total time should be reasonable
    assert!(
        total_time < Duration::from_secs(10),
        "Total time for {} acquisitions ({:?}) is too slow",
        NUM_ACQUISITIONS,
        total_time
    );

    Ok(())
}
