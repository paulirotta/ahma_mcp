mod common;

use anyhow::Result;
use std::time::Instant;

#[tokio::test]
async fn test_await_blocks_correctly() -> Result<()> {
    let (_temp_dir, mut client) = common::test_utils::setup_mcp_service_with_client().await?;

    // Start a long-running asynchronous task (e.g., sleep for 2 seconds)
    let start_time = Instant::now();
    let long_running_task = client.long_running_async("2").await?;
    assert_eq!(
        long_running_task.status, "started",
        "Task should be in 'started' state initially."
    );
    println!("Started operation: {}", long_running_task.op_id);

    // A single call to await should now block until the operation is complete.
    let await_start = Instant::now();
    let await_result = client.await_op(&long_running_task.op_id).await?;
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

    // The result of the await should contain the completed operation's status.
    assert!(
        await_result.contains(&long_running_task.op_id),
        "Await result should contain the operation ID."
    );
    assert!(
        await_result.contains("completed"),
        "Await result should indicate completion."
    );

    // For good measure, check the status tool again.
    let final_status = client.status(&long_running_task.op_id).await?;

    // The task should now be 'completed'.
    assert_eq!(
        final_status.status, "completed",
        "The await tool did not block until the operation was complete."
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
