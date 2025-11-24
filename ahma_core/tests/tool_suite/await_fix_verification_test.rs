use ahma_core::client::Client;
use anyhow::Result;
use std::time::{Duration, Instant};
use tempfile::TempDir;

async fn setup_mcp_service_with_long_running_tool() -> Result<(TempDir, Client)> {
    // Create a temporary directory for tool configs
    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path();
    let tool_config_path = tools_dir.join("shell_async.json");

    let tool_config_content = r#"
    {
        "name": "shell_async",
        "description": "Execute shell commands asynchronously. Runs in background. Returns operation_id immediately. Results pushed via notification when complete. Continue with other tasks.",
        "command": "bash -c",
        "timeout_seconds": 30,
        "enabled": true,
        "subcommand": [
            {
                "name": "default",
                "force_synchronous": false,
                "description": "Execute a shell command asynchronously. Runs in background. Returns operation_id immediately. Results delivered via notification when complete. Continue with other tasks.",
                "positional_args": [
                    {
                        "name": "command",
                        "type": "string",
                        "description": "shell command to execute",
                        "required": true
                    }
                ]
            }
        ]
    }
    "#;
    std::fs::write(&tool_config_path, tool_config_content)?;

    let mut client = Client::new();
    // Start with --async flag to enable async execution
    client
        .start_process_with_args(Some(tools_dir.to_str().unwrap()), &["--async"])
        .await?;

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    Ok((temp_dir, client))
}

#[tokio::test]
async fn test_await_blocks_correctly() -> Result<()> {
    let (_temp_dir, mut client) = setup_mcp_service_with_long_running_tool().await?;

    // Start a long-running asynchronous task (e.g., sleep for 2 seconds)
    let start_time = Instant::now();
    let long_running_task = client.shell_async_sleep("2").await?;
    assert_eq!(
        long_running_task.status, "started",
        "Task should be in 'started' state initially."
    );
    println!("Started operation: {}", long_running_task.job_id);

    // A single call to await should now block until the operation is complete.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let await_start = Instant::now();
    let await_result = client.await_op(&long_running_task.job_id).await?;
    let await_duration = await_start.elapsed();

    println!("Await returned: {}", await_result);
    println!("Await took: {:?}", await_duration);

    // The await should have taken at least 1.5 seconds (allowing some margin)
    // If the bug was present, await would return immediately
    assert!(
        await_duration.as_secs_f64() >= 1.5,
        "Await should have blocked for at least 1.5 seconds, but returned in {:?}",
        await_duration
    );

    // The result of the await should indicate successful completion.
    assert!(
        await_result.contains("Completed") || await_result.contains("completed"),
        "Await result should indicate completion. Got: {}",
        await_result
    );
    assert!(
        await_result.contains("operations") || await_result.contains("operation"),
        "Await result should reference operations. Got: {}",
        await_result
    );

    // For good measure, check the status tool again.
    let final_status_text = client.status(&long_running_task.job_id).await?;

    // The task should now be 'completed'.
    assert!(
        final_status_text.contains("completed") || final_status_text.contains("Operation"),
        "The await tool did not block until the operation was complete. Status: {}",
        final_status_text
    );

    // Total time should be close to 2 seconds (the sleep duration)
    let total_duration = start_time.elapsed();
    assert!(
        total_duration.as_secs_f64() >= 1.8 && total_duration.as_secs_f64() <= 4.0,
        "Total operation time should be close to 2 seconds, was {:?}",
        total_duration
    );

    println!("✅ Await tool correctly blocked until operation completed");
    Ok(())
}

#[tokio::test]
async fn test_await_detects_pending_operation_without_delay() -> Result<()> {
    let (_temp_dir, mut client) = setup_mcp_service_with_long_running_tool().await?;

    // Launch an async operation and immediately await it.
    let long_running_task = client.shell_async_sleep("1").await?;
    assert_eq!(long_running_task.status, "started");

    let await_start = Instant::now();
    let await_result = client.await_op(&long_running_task.job_id).await?;
    let await_duration = await_start.elapsed();

    assert!(
        await_duration.as_secs_f64() >= 0.8,
        "Await returned too quickly ({}s) indicating the operation was not detected as pending. Result: {}",
        await_duration.as_secs_f64(),
        await_result
    );

    assert!(
        await_result.contains("operation") || await_result.contains("completed"),
        "Await result should reference the operation completion. Got: {}",
        await_result
    );

    Ok(())
}
