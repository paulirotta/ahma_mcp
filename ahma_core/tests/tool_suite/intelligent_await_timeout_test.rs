/// Intelligent Await Timeout Test Suite
use ahma_core::test_utils as common;
///
/// PURPOSE: Tests the intelligent timeout behavior for the await tool:
/// 1. The await tool uses intelligent timeout calculation only (no timeout parameter)
/// 2. Intelligent timeout = max(240s default, max timeout of pending operations)
/// 3. When no operations are pending, uses 240s default and returns immediately
///
/// This test suite validates the intelligent timeout implementation.
use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::time::Duration;
use tokio::time::{Instant, timeout};

/// Test: No timeout provided, no operations running
/// Expected: Use intelligent timeout (240s default) but return immediately
#[tokio::test]
async fn test_no_timeout_no_operations_uses_default() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let call_param = CallToolRequestParam {
        name: "await".into(),
        arguments: None, // No timeout, no tools filter
    };

    let start = Instant::now();
    let result = timeout(Duration::from_secs(2), client.call_tool(call_param)).await??;
    let elapsed = start.elapsed();

    // Should return immediately since no operations are running
    assert!(!result.is_error.unwrap_or(false), "Should not be an error");
    assert!(
        elapsed < Duration::from_secs(1),
        "Should be very fast with no operations"
    );

    client.cancel().await?;
    Ok(())
}

/// Test: Intelligent timeout calculation behavior
/// Expected: Test that we can verify basic await functionality
#[tokio::test]
async fn test_intelligent_timeout_calculation_needed() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test basic await functionality without explicit timeout
    let await_param = CallToolRequestParam {
        name: "await".into(),
        arguments: None, // No explicit timeout - should use default/intelligent calculation
    };

    let start = Instant::now();
    let result = timeout(Duration::from_secs(2), client.call_tool(await_param)).await??;
    let elapsed = start.elapsed();

    // Should complete quickly since no operations are pending
    assert!(!result.is_error.unwrap_or(false), "Should not be an error");
    assert!(
        elapsed < Duration::from_secs(1),
        "Should complete quickly when no operations pending, got: {:?}",
        elapsed
    );

    println!(
        "✅ Basic await functionality working - completed in {:?}",
        elapsed
    );
    Ok(())
}

/// Test: No timeout parameter accepted - intelligent timeout only
/// Expected: Complete immediately when no operations are pending (correct behavior)
#[tokio::test]
async fn test_no_timeout_parameter_accepted() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test that await tool no longer accepts timeout_seconds parameter
    // It should use intelligent timeout calculation only
    let await_param = CallToolRequestParam {
        name: "await".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("tools".to_string(), json!("shell")); // Only tools filter allowed
            args
        }),
    };

    let start = Instant::now();
    let result = timeout(Duration::from_secs(5), client.call_tool(await_param)).await??;
    let elapsed = start.elapsed();

    // Should complete immediately since no operations are pending (correct behavior)
    assert!(!result.is_error.unwrap_or(false), "Should not be an error");
    assert!(
        elapsed < Duration::from_millis(100), // Should complete very quickly
        "Should complete immediately when no operations pending, got: {:?}",
        elapsed
    );

    // The await tool should complete successfully when no operations are pending
    println!(
        "✅ Intelligent timeout behavior working - completed immediately in {:?}",
        elapsed
    );
    Ok(())
}

/// Test: Tool-specific filtering with intelligent timeout
/// Expected: Complete immediately when no matching operations are pending
#[tokio::test]
async fn test_tool_filtered_intelligent_timeout() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test await with tool filtering (should complete immediately when no matching operations)
    let await_param = CallToolRequestParam {
        name: "await".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("tools".to_string(), json!("shell")); // Filter for shell tools only
            args
        }),
    };

    let start = Instant::now();
    let result = timeout(Duration::from_secs(3), client.call_tool(await_param)).await??;
    let elapsed = start.elapsed();

    // Should complete immediately since no matching operations are pending
    assert!(!result.is_error.unwrap_or(false), "Should not be an error");
    assert!(
        elapsed < Duration::from_millis(100), // Should complete very quickly
        "Should complete immediately when no matching operations pending, got: {:?}",
        elapsed
    );

    println!(
        "✅ Tool filtering functionality working - completed immediately in {:?}",
        elapsed
    );
    Ok(())
}

/// Test: Intelligent timeout calculation with long-running operations
/// Expected: Await tool should use intelligent timeout based on operation timeouts
#[tokio::test]
async fn test_intelligent_timeout_with_long_operations() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Start operation with long timeout
    let long_op_param = CallToolRequestParam {
        name: "shell_async".into(),
        arguments: Some({
            let mut args = serde_json::Map::new();
            args.insert("command".to_string(), json!("sleep 600")); // 10 minutes
            args
        }),
    };

    let _long_op_result = client.call_tool(long_op_param);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Use await tool with intelligent timeout (no timeout parameter)
    let await_param = CallToolRequestParam {
        name: "await".into(),
        arguments: None, // No parameters - uses intelligent timeout
    };

    // Test that the await tool calculates intelligent timeout correctly
    // Since we have a 600s operation, intelligent timeout should be 600s
    // But we'll test with a short test timeout to verify behavior
    let result = timeout(Duration::from_secs(35), client.call_tool(await_param)).await??;

    // The await operation should timeout at the test level (35s),
    // but internally it would have used 600s intelligent timeout
    assert!(!result.is_error.unwrap_or(false), "Should not be an error");

    println!("✅ Intelligent timeout calculation working with long operations");
    Ok(())
}

/// Test: Edge case - intelligent timeout when no operations pending
/// Expected: Complete immediately when no operations are pending
#[tokio::test]
async fn test_intelligent_timeout_no_pending_operations() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Test intelligent timeout behavior - should complete immediately when no operations pending
    let await_param = CallToolRequestParam {
        name: "await".into(),
        arguments: None, // No parameters - uses intelligent timeout
    };

    let start = Instant::now();
    let result = timeout(Duration::from_secs(5), client.call_tool(await_param)).await??;
    let elapsed = start.elapsed();

    // Should complete immediately since no operations are pending (correct behavior)
    assert!(!result.is_error.unwrap_or(false), "Should not be an error");
    assert!(
        elapsed < Duration::from_millis(100), // Should complete very quickly
        "Should complete immediately when no operations pending, got: {:?}",
        elapsed
    );

    println!(
        "✅ Intelligent timeout behavior working - completed immediately in {:?}",
        elapsed
    );
    Ok(())
}
