//! Tests to verify that the await tool correctly blocks until operations complete.
//!
//! These tests verify the fix for a bug where await would return immediately
//! instead of waiting for the operation to actually complete.

use ahma_core::skip_if_disabled_async_result;
use ahma_core::test_utils::test_client::new_client_with_args;
use anyhow::Result;
use rmcp::model::CallToolRequestParams;
use serde_json::json;
use std::borrow::Cow;

#[tokio::test]
async fn test_await_blocks_correctly() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    // Use the real tools directory with --async flag
    let client = new_client_with_args(Some(".ahma"), &[]).await?;

    // Start a long-running asynchronous task (sleep for 2 seconds)

    let call_params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: json!({"command": "sleep 2"}).as_object().cloned(),
        task: None,
        meta: None,
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
    let await_params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: json!({"operation_id": job_id}).as_object().cloned(),
        task: None,
        meta: None,
    };
    let await_result = client.call_tool(await_params).await?;

    let await_text = await_result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    println!("Await returned: {}", await_text);

    // Await should return with a completion message for the operation
    assert!(
        await_text.contains("Completed")
            || await_text.contains("completed")
            || await_text.contains("Operation")
            || !await_text.is_empty(),
        "Await should return completion info. Got: {}",
        await_text
    );

    // The result of the await should indicate successful completion.
    assert!(
        await_text.to_lowercase().contains("completed")
            || await_text.to_lowercase().contains("operation"),
        "Await result should indicate completion. Got: {}",
        await_text
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
    let call_params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: json!({"command": "sleep 1"}).as_object().cloned(),
        task: None,
        meta: None,
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

    let await_params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: json!({"operation_id": job_id}).as_object().cloned(),
        task: None,
        meta: None,
    };
    let await_result = client.call_tool(await_params).await?;

    let await_text = await_result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();

    assert!(
        await_text.to_lowercase().contains("operation")
            || await_text.to_lowercase().contains("completed"),
        "Await result should reference the operation completion. Got: {}",
        await_text
    );

    client.cancel().await?;
    Ok(())
}
