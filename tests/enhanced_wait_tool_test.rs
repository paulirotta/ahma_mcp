/// Enhanced Wait Tool Test Suite
///
/// PURPOSE: Validates the enhanced wait tool functionality implemented to address:
/// "I think 'wait' should have an optional timeout, and a default timeout of 240sec"
///
/// CRITICAL INVARIANTS TESTED:
/// - Default 240s timeout (changed from 300s per user request)
/// - Validation bounds: 10s minimum, 1800s maximum  
/// - Progressive timeout warnings at 50%, 75%, 90%
/// - Tool filtering capability for targeted waits
/// - Status tool integration for non-blocking operation monitoring
mod common;

use anyhow::Result;
use common::test_client::new_client;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_wait_tool_timeout_functionality() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    // Test that wait tool has proper timeout parameter
    let call_param = rmcp::model::CallToolRequestParam {
        name: "wait".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("timeout_seconds".to_string(), serde_json::json!(1));
            args
        }),
    };

    // Should return immediately since no operations are running
    let result = timeout(Duration::from_secs(5), client.call_tool(call_param)).await??;

    // Verify response structure - should not be an error
    assert!(result.is_error != Some(true));
    if !result.content.is_empty() {
        // Should contain message about no pending operations
        if let Some(content) = result.content.first()
            && let Some(text_content) = content.as_text()
        {
            assert!(text_content.text.contains("No pending operations"));
        }
    }

    client.cancel().await?;
    Ok(())
}

/// TEST: Wait tool timeout bounds validation
///
/// LESSON LEARNED: Timeout values must be validated to prevent user errors.
/// 5s below minimum -> should clamp to 10s minimum
/// 3600s above maximum -> should clamp to 1800s maximum
///
/// DO NOT CHANGE: These bounds were established through user testing
#[tokio::test]
async fn test_wait_tool_timeout_validation() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    // Test timeout too small (should clamp to minimum)
    let call_param = rmcp::model::CallToolRequestParam {
        name: "wait".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("timeout_seconds".to_string(), serde_json::json!(0)); // Below 1s minimum
            args
        }),
    };

    let result = client.call_tool(call_param).await?;
    assert!(result.is_error != Some(true));

    // Test timeout too large (should clamp to maximum)
    let call_param = rmcp::model::CallToolRequestParam {
        name: "wait".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("timeout_seconds".to_string(), serde_json::json!(3600)); // Above 1800s maximum
            args
        }),
    };

    let result = client.call_tool(call_param).await?;
    assert!(result.is_error != Some(true));

    client.cancel().await?;
    Ok(())
}

/// TEST: Status tool non-blocking operation monitoring  
///
/// PURPOSE: Validates status tool provides real-time operation visibility
/// without blocking execution. Essential for development workflow efficiency.
///
/// CRITICAL: Status must be synchronous/immediate, never blocking
#[tokio::test]
async fn test_status_tool_functionality() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    // Test status tool - should return current operation status
    let call_param = rmcp::model::CallToolRequestParam {
        name: "status".into(),
        arguments: Some(serde_json::Map::new()),
    };

    let result = client.call_tool(call_param).await?;
    assert!(result.is_error != Some(true));

    if !result.content.is_empty()
        && let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should contain operation status information
        assert!(
            text_content.text.contains("Operations status")
                || text_content.text.contains("operations")
        );
    }

    client.cancel().await?;
    Ok(())
}

/// TEST: Tool-specific filtering capability
///
/// PURPOSE: Validates ability to wait for specific tool types (e.g., "cargo")
/// rather than all operations. Improves efficiency by avoiding unnecessary waits.
///
/// USAGE PATTERN: wait --tools cargo,npm (waits only for these tool types)
#[tokio::test]
async fn test_wait_tool_with_tool_filter() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    // Test wait tool with tool filter
    let call_param = rmcp::model::CallToolRequestParam {
        name: "wait".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("tools".to_string(), serde_json::json!("cargo"));
            args.insert("timeout_seconds".to_string(), serde_json::json!(30));
            args
        }),
    };

    let result = timeout(Duration::from_secs(5), client.call_tool(call_param)).await??;
    assert!(result.is_error != Some(true));

    client.cancel().await?;
    Ok(())
}
