//! Tests to verify that the await tool correctly blocks until operations complete.
//!
//! These tests verify the fix for a bug where await would return immediately
//! instead of waiting for the operation to actually complete.

use ahma_core::skip_if_disabled_async_result;
use ahma_core::test_utils::test_client::new_client_with_args;
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_await_blocks_correctly() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    // Use the real tools directory with --async flag
    let client = new_client_with_args(Some(".ahma"), &[]).await?;

    // Start a long-running asynchronous task (sleep for 2 seconds)
    let start_time = Instant::now();

    let call_params = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: json!({"command": "sleep 2"}).as_object().cloned(),
        task: None,
    };
    let result = client.call_tool(call_params).await?;

    // Extract operation ID from the response
    let response_text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    assert!(
        response_text.contains("ID:"),
        "Should return operation ID. Got: {}",
        response_text
    );

    // Extract the operation ID
    let job_id = response_text
        .split("ID: ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .ok_or_else(|| anyhow::anyhow!("Could not extract operation ID from: {}", response_text))?;

    println!("Started operation: {}", job_id);

    // A single call to await should block until the operation is complete.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let await_start = Instant::now();

    let await_params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: json!({"operation_id": job_id}).as_object().cloned(),
        task: None,
    };
    let await_result = client.call_tool(await_params).await?;
    let await_duration = await_start.elapsed();

    let await_text = await_result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    println!("Await returned: {}", await_text);
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
        await_text.to_lowercase().contains("completed")
            || await_text.to_lowercase().contains("operation"),
        "Await result should indicate completion. Got: {}",
        await_text
    );

    // Total time should be close to 2 seconds (the sleep duration)
    let total_duration = start_time.elapsed();
    assert!(
        total_duration.as_secs_f64() >= 1.8 && total_duration.as_secs_f64() <= 5.0,
        "Total operation time should be close to 2 seconds, was {:?}",
        total_duration
    );

    println!("✅ Await tool correctly blocked until operation completed");
    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_await_detects_pending_operation_without_delay() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    // Use the real tools directory with --async flag
    let client = new_client_with_args(Some(".ahma"), &[]).await?;

    // Launch an async operation and immediately await it.
    let call_params = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: json!({"command": "sleep 1"}).as_object().cloned(),
        task: None,
    };
    let result = client.call_tool(call_params).await?;

    let response_text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    let job_id = response_text
        .split("ID: ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .ok_or_else(|| anyhow::anyhow!("Could not extract operation ID"))?;

    let await_start = Instant::now();

    let await_params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: json!({"operation_id": job_id}).as_object().cloned(),
        task: None,
    };
    let await_result = client.call_tool(await_params).await?;
    let await_duration = await_start.elapsed();

    let await_text = await_result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    assert!(
        await_duration.as_secs_f64() >= 0.8,
        "Await returned too quickly ({}s) indicating the operation was not detected as pending. Result: {}",
        await_duration.as_secs_f64(),
        await_text
    );

    assert!(
        await_text.to_lowercase().contains("operation")
            || await_text.to_lowercase().contains("completed"),
        "Await result should reference the operation completion. Got: {}",
        await_text
    );

    client.cancel().await?;
    Ok(())
}
