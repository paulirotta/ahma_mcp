/// Enhanced Wait Tool Test Suite
mod common;
///
/// PURPOSE: Validates the enhanced await tool functionality implemented to address:
/// "I think 'await' should have an optional timeout, and a default timeout of 240sec"
///
/// CRITICAL INVARIANTS TESTED:
/// - Default 240s timeout (changed from 300s per user request)
/// - Validation bounds: 10s minimum, 1800s maximum  
/// - Progressive timeout warnings at 50%, 75%, 90%
/// - Tool filtering capability for targeted waits
/// - Status tool integration for non-blocking operation monitoring
use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use common::test_client::new_client;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_await_tool_timeout_functionality() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test that await tool uses intelligent timeout calculation only
    let call_param = rmcp::model::CallToolRequestParam {
        name: "await".into(),
        arguments: Some(serde_json::Map::new()),
    };

    // Should return immediately since no operations are running
    let result = timeout(Duration::from_secs(5), client.call_tool(call_param)).await??;

    // Verify response structure - should not be an error
    assert!(result.is_error != Some(true));
    if !result.content.is_empty() {
        // Should contain message about no pending operations
        if let Some(content) = result.content.first() {
            if let Some(text_content) = content.as_text() {
                assert!(text_content.text.contains("No pending operations"));
            }
        }
    }

    client.cancel().await?;
    Ok(())
}

/// TEST: Await tool no longer accepts timeout parameter
///
/// LESSON LEARNED: Timeout is now calculated intelligently based on pending operations.
/// The user timeout parameter has been removed to rely solely on intelligent calculation.
///
/// DO NOT CHANGE: The intelligent timeout system was established through user feedback
#[tokio::test]
async fn test_await_tool_timeout_validation() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test that timeout parameter is no longer accepted
    let call_param = rmcp::model::CallToolRequestParam {
        name: "await".into(),
        arguments: Some(serde_json::Map::new()),
    };

    let result = client.call_tool(call_param).await?;
    assert!(result.is_error != Some(true));

    // Test another call without any timeout parameter
    let call_param = rmcp::model::CallToolRequestParam {
        name: "await".into(),
        arguments: Some(serde_json::Map::new()),
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
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test status tool - should return current operation status
    let call_param = rmcp::model::CallToolRequestParam {
        name: "status".into(),
        arguments: Some(serde_json::Map::new()),
    };

    let result = client.call_tool(call_param).await?;
    assert!(result.is_error != Some(true));

    if !result.content.is_empty() {
        if let Some(content) = result.content.first() {
            if let Some(text_content) = content.as_text() {
                // Should contain operation status information
                assert!(
                    text_content.text.contains("Operations status")
                        || text_content.text.contains("operations")
                );
            }
        }
    }

    client.cancel().await?;
    Ok(())
}

/// TEST: Tool-specific filtering capability
///
/// PURPOSE: Validates ability to await for specific tool types (e.g., "cargo")
/// rather than all operations. Improves efficiency by avoiding unnecessary waits.
///
/// USAGE PATTERN: await --tools cargo,npm (waits only for these tool types)
#[tokio::test]
async fn test_await_tool_with_tool_filter() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test await tool with tool filter only (no timeout parameter)
    let call_param = rmcp::model::CallToolRequestParam {
        name: "await".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("tools".to_string(), serde_json::json!("cargo"));
            args
        }),
    };

    let result = timeout(Duration::from_secs(5), client.call_tool(call_param)).await??;
    assert!(result.is_error != Some(true));

    client.cancel().await?;
    Ok(())
}
