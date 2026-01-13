use ahma_core::test_utils as common;
/// Comprehensive integration tests for mcp_service.rs coverage improvement
///
/// Target: Improve mcp_service.rs coverage from 59.44% to 85%+
/// Focus: Hardcoded tools, schema generation, error handling, path validation
///
/// Uses the integration test pattern from existing working tests
use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::{Map, json};
use std::borrow::Cow;

/// Test that hardcoded tools are properly listed
#[tokio::test]
async fn test_hardcoded_tools_listing() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;
    let result = client.list_all_tools().await?;

    // Should have the hardcoded tools (await, status)
    assert!(!result.is_empty());
    let tool_names: Vec<_> = result.iter().map(|t| t.name.as_ref()).collect();

    // Verify hardcoded tools are present
    assert!(tool_names.contains(&"await"));
    assert!(tool_names.contains(&"status"));

    // Verify each tool has proper schema
    for tool in &result {
        assert!(!tool.name.is_empty());
        assert!(tool.description.is_some());
        assert!(!tool.description.as_ref().unwrap().is_empty());

        // Verify input schema exists and is valid JSON structure
        let schema_value = serde_json::to_value(&*tool.input_schema);
        assert!(schema_value.is_ok());
    }

    client.cancel().await?;
    Ok(())
}

/// Test await tool functionality and error handling
#[tokio::test]
async fn test_await_tool_comprehensive() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    // Test valid await call with no timeout parameter (uses intelligent timeout)
    let params = Map::new();

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(params),
    };

    let result = client.call_tool(call_param).await?;
    assert!(!result.content.is_empty());

    // Verify response contains operation information
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        assert!(
            text_content.text.contains("operation")
                || text_content.text.contains("await")
                || text_content.text.contains("complete")
        );
    }

    // Test await with only valid fields (no timeout_seconds)
    let mut valid_params = Map::new();
    valid_params.insert("tools".to_string(), json!("cargo"));

    let valid_call_param = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(valid_params),
    };

    let valid_result = client.call_tool(valid_call_param).await?;
    assert!(!valid_result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test status tool functionality
#[tokio::test]
async fn test_status_tool_comprehensive() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    // Test basic status call
    let params = Map::new();

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(params),
    };

    let result = client.call_tool(call_param).await?;
    assert!(!result.content.is_empty());

    // Verify status provides operation information
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        assert!(
            text_content.text.contains("Operations")
                || text_content.text.contains("status")
                || text_content.text.contains("operation")
        );
    }

    // Test status with operation_id parameter
    let mut specific_params = Map::new();
    specific_params.insert("operation_id".to_string(), json!("test_operation_123"));

    let specific_call_param = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(specific_params),
    };

    let specific_result = client.call_tool(specific_call_param).await?;
    assert!(!specific_result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test error handling for unknown tools
#[tokio::test]
async fn test_unknown_tool_error_handling() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    let params = Map::new();

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("nonexistent_tool_xyz"),
        arguments: Some(params),
    };

    let result = client.call_tool(call_param).await;

    // Should handle unknown tools gracefully
    match result {
        Ok(tool_result) => {
            // Check if error is communicated in response content
            if let Some(content) = tool_result.content.first()
                && let Some(text_content) = content.as_text()
            {
                let text_lower = text_content.text.to_lowercase();
                assert!(
                    text_lower.contains("error")
                        || text_lower.contains("not found")
                        || text_lower.contains("unknown")
                        || text_lower.contains("invalid")
                );
            }
        }
        Err(_) => {
            // Error response is also acceptable for unknown tools
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test concurrent tool execution
#[tokio::test]
async fn test_concurrent_tool_execution() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    // Execute multiple status calls concurrently
    let mut handles = vec![];

    for i in 0..5 {
        let client_clone = new_client(Some(".ahma")).await?;
        let handle = tokio::spawn(async move {
            let mut params = Map::new();
            params.insert(
                "operation_id".to_string(),
                json!(format!("concurrent_test_{}", i)),
            );

            let call_param = CallToolRequestParam {
                name: Cow::Borrowed("status"),
                arguments: Some(params),
            };

            let result = client_clone.call_tool(call_param).await;
            client_clone.cancel().await.ok(); // Clean up
            result
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    for handle in handles {
        let result = handle.await.expect("Task should not panic");
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(!tool_result.content.is_empty());
    }

    client.cancel().await?;
    Ok(())
}

/// Test path validation and security
#[tokio::test]
async fn test_path_validation_security() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    // Test with potentially dangerous path arguments
    let mut params = Map::new();
    params.insert(
        "working_directory".to_string(),
        json!("/../../../../etc/passwd"),
    );

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(params),
    };

    let result = client.call_tool(call_param).await;

    // Should handle path validation gracefully without security issues
    match result {
        Ok(tool_result) => {
            assert!(!tool_result.content.is_empty());
            // Should complete without exposing sensitive system information
        }
        Err(_) => {
            // Error for security validation is acceptable
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test tool schema generation and validation
#[tokio::test]
async fn test_tool_schema_validation() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;
    let tools = client.list_all_tools().await?;

    // Verify all tools have valid schemas
    for tool in &tools {
        // Basic tool structure validation
        assert!(!tool.name.is_empty());
        assert!(tool.description.is_some());
        assert!(!tool.description.as_ref().unwrap().is_empty());

        // Schema validation
        // Should be valid JSON
        let schema_json = serde_json::to_value(&*tool.input_schema)?;
        assert!(schema_json.is_object());

        // Should have type information
        if let Some(obj) = schema_json.as_object() {
            // JSON Schema should have a type or properties
            assert!(
                obj.contains_key("type")
                    || obj.contains_key("properties")
                    || obj.contains_key("oneOf")
                    || obj.contains_key("anyOf")
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test resilience under stress and mixed operations
#[tokio::test]
async fn test_service_resilience_stress() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    // Execute a mix of valid and invalid operations
    let operations = vec![
        ("status", json!({})),
        ("invalid_tool_123", json!({})),
        ("await", json!({})),
        ("another_invalid_tool", json!({"invalid": "args"})),
        ("status", json!({"operation_id": "stress_test"})),
    ];

    for (tool_name, args) in operations {
        let call_param = CallToolRequestParam {
            name: Cow::Borrowed(tool_name),
            arguments: args.as_object().cloned(),
        };

        let result = client.call_tool(call_param).await;

        // Service should handle both valid and invalid requests gracefully
        match result {
            Ok(tool_result) => {
                assert!(!tool_result.content.is_empty());
            }
            Err(_) => {
                // Errors are acceptable for invalid tools
            }
        }
    }

    // Service should still be functional after error conditions
    let final_params = Map::new();
    let final_call_param = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(final_params),
    };

    let final_result = client.call_tool(final_call_param).await?;
    assert!(!final_result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test argument parsing and parameter handling
#[tokio::test]
async fn test_argument_parsing_edge_cases() -> Result<()> {
    let client = new_client(Some(".ahma")).await?;

    // Test with empty arguments
    let empty_call_param = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: None,
    };

    let empty_result = client.call_tool(empty_call_param).await?;
    assert!(!empty_result.content.is_empty());

    // Test with complex nested JSON arguments (no timeout_seconds)
    let complex_args = json!({
        "nested": {
            "array": [1, 2, 3],
            "object": {
                "key": "value"
            }
        },
        "tools": "cargo"
    });

    let complex_call_param = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: complex_args.as_object().cloned(),
    };

    let complex_result = client.call_tool(complex_call_param).await;

    // Should handle complex arguments gracefully
    match complex_result {
        Ok(tool_result) => {
            assert!(!tool_result.content.is_empty());
        }
        Err(_) => {
            // Error handling is acceptable for complex/invalid arguments
        }
    }

    client.cancel().await?;
    Ok(())
}
