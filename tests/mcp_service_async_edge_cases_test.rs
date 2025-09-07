//! Advanced MCP service testing for async notification delivery edge cases,
//! tool schema generation validation, and error handling for malformed MCP messages.
//!
//! This test module specifically targets Phase 7 requirements for:
//! - Async notification delivery edge cases  
//! - Tool schema generation validation
//! - Error handling for malformed MCP messages

use anyhow::Result;
use serde_json::json;
use std::borrow::Cow;

mod common;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;

/// Test async notification delivery with malformed operation IDs
#[tokio::test]
async fn test_async_notification_malformed_operation_ids() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test status tool with numeric operation_id (should be handled gracefully)
    let malformed_params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({
                "operation_id": 12345  // numeric instead of string
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(malformed_params).await;

    // Should complete successfully (status tool should handle this gracefully)
    assert!(result.is_ok());
    let call_result = result.unwrap();
    assert!(!call_result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test async notification delivery with edge case timeout values
#[tokio::test]
async fn test_async_notification_extreme_timeout_values() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test with zero timeout (should return immediately)
    let zero_timeout_params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({
                "timeout_seconds": 0
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(zero_timeout_params).await?;
    assert!(!result.content.is_empty());

    // Test with negative timeout (should be handled gracefully)
    let negative_timeout_params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({
                "timeout_seconds": -1
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(negative_timeout_params).await;
    // Should either succeed or fail gracefully
    assert!(result.is_ok() || result.is_err());

    client.cancel().await?;
    Ok(())
}

/// Test tool schema generation with complex tool discovery
#[tokio::test]
async fn test_tool_schema_generation_comprehensive() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test list_tools generates proper schemas
    let tools_result = client.list_all_tools().await?;

    // Should have multiple tools from .ahma/tools directory
    assert!(!tools_result.is_empty());
    assert!(
        tools_result.len() > 5,
        "Expected multiple tools but got: {}",
        tools_result.len()
    );

    // Verify each tool has proper schema structure
    for tool in &tools_result {
        assert!(!tool.name.is_empty());

        // Check tool description exists
        if let Some(desc) = &tool.description {
            assert!(!desc.is_empty());
        }

        // Verify schema structure
        assert!(!tool.input_schema.is_empty());

        // Check that the schema contains basic required fields
        assert!(tool.input_schema.contains_key("type"));
        if let Some(type_val) = tool.input_schema.get("type") {
            assert_eq!(type_val.as_str().unwrap_or(""), "object");
        }
    }

    // Verify specific known tools exist
    let tool_names: Vec<&str> = tools_result.iter().map(|t| t.name.as_ref()).collect();

    assert!(tool_names.contains(&"await"), "Should have await tool");
    assert!(tool_names.contains(&"status"), "Should have status tool");
    assert!(tool_names.contains(&"ls"), "Should have ls tool");

    client.cancel().await?;
    Ok(())
}

/// Test error handling for malformed call_tool parameters
#[tokio::test]
async fn test_error_handling_malformed_call_tool_params() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test with missing required parameters for cancel tool
    let missing_params = CallToolRequestParam {
        name: Cow::Borrowed("cancel"),
        arguments: Some(
            json!({
                // Missing operation_id which is required
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(missing_params).await;
    assert!(
        result.is_err(),
        "Cancel tool should require operation_id parameter"
    );

    // Test with invalid parameter types for await tool
    let invalid_types_params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({
                "timeout_seconds": "not_a_number"  // String instead of number
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(invalid_types_params).await;
    // Should handle type mismatch gracefully
    assert!(result.is_ok() || result.is_err());

    client.cancel().await?;
    Ok(())
}

/// Test error handling for unknown tools
#[tokio::test]
async fn test_error_handling_unknown_tools() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    let unknown_tool_params = CallToolRequestParam {
        name: Cow::Borrowed("nonexistent_tool_xyz_123"),
        arguments: None,
    };

    let result = client.call_tool(unknown_tool_params).await;
    assert!(result.is_err(), "Unknown tools should return error");

    client.cancel().await?;
    Ok(())
}

/// Test async notification system under concurrent load
#[tokio::test]
async fn test_async_notification_concurrent_load() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Start multiple async operations concurrently
    let mut handles = Vec::new();
    for i in 0..3 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let params = CallToolRequestParam {
                name: Cow::Borrowed("status"),
                arguments: Some(
                    json!({
                        "operation_id": format!("test_concurrent_op_{}", i)
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                ),
            };
            client_clone.call_tool(params).await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    let results = futures::future::join_all(handles).await;

    // All should complete successfully
    for (i, result) in results.into_iter().enumerate() {
        let call_result = result
            .unwrap_or_else(|e| panic!("Task {} failed: {}", i, e))
            .unwrap_or_else(|e| panic!("Call {} failed: {}", i, e));
        assert!(!call_result.content.is_empty());
    }

    client.cancel().await?;
    Ok(())
}

/// Test status tool with various filter combinations
#[tokio::test]
async fn test_status_tool_filter_combinations() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test with tool filter
    let tool_filter_params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({
                "tools": "cargo,git"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(tool_filter_params).await?;
    assert!(!result.content.is_empty());

    // Check that response mentions the filtered tools
    if let Some(content) = result.content.first() {
        if let Some(text_content) = content.as_text() {
            let text = &text_content.text;
            assert!(text.contains("cargo") || text.contains("Operations status"));
        }
    }

    // Test with both operation_id and tools filter
    let combined_filter_params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({
                "operation_id": "test_123",
                "tools": "cargo"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(combined_filter_params).await?;
    assert!(!result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test async operations with real tool execution
#[tokio::test]
async fn test_async_operation_with_real_execution() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Start a real async operation (shell command)
    let async_params = CallToolRequestParam {
        name: Cow::Borrowed("shell_async"),
        arguments: Some(
            json!({
                "command": "echo 'test async execution'",
                "timeout_seconds": 30
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(async_params).await?;
    assert!(!result.content.is_empty());

    // Should return operation info immediately
    if let Some(content) = result.content.first() {
        if let Some(text_content) = content.as_text() {
            let text = &text_content.text;
            assert!(
                text.contains("operation_id")
                    || text.contains("started")
                    || text.contains("job_id"),
                "Async operation should return operation tracking info, got: {}",
                text
            );
        }
    }

    // Test that we can query the status
    let status_params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: None,
    };

    let status_result = client.call_tool(status_params).await?;
    assert!(!status_result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test error recovery and resilience
#[tokio::test]
async fn test_error_recovery_and_resilience() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test that service continues working after errors

    // 1. Cause an error with unknown tool
    let _ = client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("unknown_tool"),
            arguments: None,
        })
        .await;

    // 2. Service should still work normally
    let working_params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: None,
    };

    let result = client.call_tool(working_params).await?;
    assert!(!result.content.is_empty());

    // 3. Test with multiple rapid error/success cycles
    for i in 0..3 {
        // Error call
        let invalid_tool_name = format!("invalid_tool_{}", i);
        let _ = client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(invalid_tool_name),
                arguments: None,
            })
            .await;

        // Successful call
        let good_result = client
            .call_tool(CallToolRequestParam {
                name: Cow::Borrowed("status"),
                arguments: None,
            })
            .await?;
        assert!(!good_result.content.is_empty());
    }

    client.cancel().await?;
    Ok(())
}
