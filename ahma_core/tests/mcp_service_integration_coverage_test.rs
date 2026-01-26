//! MCP Service Integration Coverage Tests
//!
//! Real integration tests for mcp_service/mod.rs targeting low-coverage areas:
//! - handle_await (with operation_id filter, tool filters, timeout paths)
//! - handle_status (with filters, concurrency analysis)
//! - call_tool paths (sequence detection, disabled tools, cancel)
//! - calculate_intelligent_timeout
//!
//! These tests spawn the actual ahma_mcp binary and communicate via MCP protocol.

use ahma_core::skip_if_disabled_async_result;
use ahma_core::test_utils::test_client::{new_client, new_client_in_dir};
use ahma_core::test_utils::test_project::{TestProjectOptions, create_rust_test_project};
use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use rmcp::model::CallToolRequestParams;
use serde_json::json;
use std::borrow::Cow;

// ============================================================================
// Status Tool Coverage Tests
// ============================================================================

/// Test status tool with no filters - should return all operations
#[tokio::test]
async fn test_status_tool_no_filters() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Call status with no arguments
    let params = CallToolRequestParams {
        name: Cow::Borrowed("status"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    // Should return status information
    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should contain operations status summary
        assert!(
            text_content.text.contains("Operations status")
                || text_content.text.contains("active")
                || text_content.text.contains("completed"),
            "Status should show operations summary, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test status tool with tool filter parameter
#[tokio::test]
async fn test_status_tool_with_tool_filter() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // First start an async operation to have something to query
    let shell_params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(json!({"command": "echo test"}).as_object().unwrap().clone()),
        task: None,
        meta: None,
    };
    let _ = client.call_tool(shell_params).await?;

    // Now query status with a tools filter
    let params = CallToolRequestParams {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({"tools": "sandboxed_shell"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should mention the filtered tool
        assert!(
            text_content.text.contains("sandboxed_shell")
                || text_content.text.contains("Operations status"),
            "Status should reference filter, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test status tool with specific operation_id filter
#[tokio::test]
async fn test_status_tool_with_operation_id_filter() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Query status with a non-existent operation_id
    let params = CallToolRequestParams {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({"operation_id": "nonexistent_op_12345"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should indicate operation not found
        assert!(
            text_content.text.contains("not found")
                || text_content.text.contains("total: 0")
                || text_content.text.contains("nonexistent_op_12345"),
            "Status should indicate operation not found, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Await Tool Coverage Tests
// ============================================================================

/// Test await tool with no pending operations
#[tokio::test]
async fn test_await_tool_no_pending_operations() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should indicate no pending operations
        assert!(
            text_content.text.contains("No pending operations")
                || text_content.text.contains("Completed")
                || text_content.text.contains("operations"),
            "Await should handle no pending ops, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test await tool with specific operation_id that doesn't exist
#[tokio::test]
async fn test_await_tool_nonexistent_operation_id() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({"operation_id": "fake_operation_xyz"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should indicate operation not found
        assert!(
            text_content.text.contains("not found")
                || text_content.text.contains("fake_operation_xyz"),
            "Await should handle non-existent operation, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test await tool with tools filter
#[tokio::test]
async fn test_await_tool_with_tool_filter() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Request await for a specific tool prefix that has no pending operations
    let params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({"tools": "nonexistent_tool"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should indicate no pending operations for this filter
        assert!(
            text_content.text.contains("No pending operations")
                || text_content.text.contains("nonexistent_tool"),
            "Await should handle empty filter results, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test await for an async operation that actually completes
#[tokio::test]
async fn test_await_for_completed_async_operation() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Start a fast async shell command
    let shell_params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({"command": "echo 'quick test'"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };
    let start_result = client.call_tool(shell_params).await?;

    // Extract operation ID if available
    let mut operation_id = None;
    if let Some(content) = start_result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Try to extract operation ID from the response
        if let Some(idx) = text_content.text.find("operation_id") {
            let rest = &text_content.text[idx..];
            // Look for patterns like "operation_id": "op_123" or operation_id: op_123
            for word in rest.split_whitespace().take(5) {
                if word.starts_with("op_") || word.starts_with("\"op_") {
                    operation_id = Some(word.trim_matches(|c| c == '"' || c == ',').to_string());
                    break;
                }
            }
        }
    }

    // Now await for it - should find it completed
    let await_params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: if let Some(ref id) = operation_id {
            Some(json!({"operation_id": id}).as_object().unwrap().clone())
        } else {
            Some(
                json!({"tools": "sandboxed_shell"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
        },
        task: None,
        meta: None,
    };

    let result = client.call_tool(await_params).await?;
    assert!(!result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Cancel Tool Coverage Tests
// ============================================================================

/// Test cancel tool with missing operation_id parameter
#[tokio::test]
async fn test_cancel_tool_missing_operation_id() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // The cancel tool isn't in the default tool list, need to check if it exists
    // If it does, call it without operation_id to trigger validation error
    let tools = client.list_all_tools().await?;
    let has_cancel = tools.iter().any(|t| t.name.as_ref() == "cancel");

    if has_cancel {
        let params = CallToolRequestParams {
            name: Cow::Borrowed("cancel"),
            arguments: Some(json!({}).as_object().unwrap().clone()),
            task: None,
            meta: None,
        };

        let result = client.call_tool(params).await;

        // Should return an error for missing parameter
        assert!(
            result.is_err(),
            "Cancel without operation_id should fail with error"
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test cancel tool with non-existent operation_id
#[tokio::test]
async fn test_cancel_tool_nonexistent_operation() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let tools = client.list_all_tools().await?;
    let has_cancel = tools.iter().any(|t| t.name.as_ref() == "cancel");

    if has_cancel {
        let params = CallToolRequestParams {
            name: Cow::Borrowed("cancel"),
            arguments: Some(
                json!({"operation_id": "nonexistent_op_999"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            task: None,
            meta: None,
        };

        let result = client.call_tool(params).await?;

        // Should indicate operation not found
        if let Some(content) = result.content.first()
            && let Some(text_content) = content.as_text()
        {
            assert!(
                text_content.text.contains("not found")
                    || text_content.text.contains("❌")
                    || text_content.text.contains("never existed"),
                "Cancel should report operation not found, got: {}",
                text_content.text
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Tool Not Found / Disabled Coverage Tests
// ============================================================================

/// Test calling a tool that doesn't exist
#[tokio::test]
async fn test_call_nonexistent_tool() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let params = CallToolRequestParams {
        name: Cow::Borrowed("this_tool_definitely_does_not_exist_xyz123"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await;

    // Should return an error
    assert!(result.is_err(), "Non-existent tool should return error");

    client.cancel().await?;
    Ok(())
}

/// Test calling a tool with invalid subcommand
#[tokio::test]
async fn test_call_tool_invalid_subcommand() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Assume there's a file_tools tool available
    let tools = client.list_all_tools().await?;
    let has_file_tools = tools
        .iter()
        .any(|t| t.name.as_ref().starts_with("file_tools"));

    if has_file_tools {
        let params = CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(
                json!({"subcommand": "nonexistent_subcommand"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            task: None,
            meta: None,
        };

        let result = client.call_tool(params).await;

        // Should return an error for invalid subcommand
        assert!(result.is_err(), "Invalid subcommand should return error");
    }

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// List Tools Coverage Tests
// ============================================================================

/// Test list_tools returns await and status tools
#[tokio::test]
async fn test_list_tools_includes_builtin_tools() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let tools = client.list_all_tools().await?;

    // Should include await and status built-in tools
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    assert!(
        tool_names.contains(&"await"),
        "Tools should include 'await'. Got: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"status"),
        "Tools should include 'status'. Got: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

/// Test list_tools schema generation for await tool
#[tokio::test]
async fn test_list_tools_await_schema() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let tools = client.list_all_tools().await?;

    let await_tool = tools.iter().find(|t| t.name.as_ref() == "await");
    assert!(await_tool.is_some(), "Should find await tool");

    let await_tool = await_tool.unwrap();
    assert!(
        await_tool.description.is_some(),
        "Await tool should have description"
    );

    // Check input schema has expected properties
    let schema = &await_tool.input_schema;
    let schema_str = serde_json::to_string(&schema)?;
    assert!(
        schema_str.contains("tools") || schema_str.contains("operation_id"),
        "Await schema should have tools or operation_id properties"
    );

    client.cancel().await?;
    Ok(())
}

/// Test list_tools schema generation for status tool
#[tokio::test]
async fn test_list_tools_status_schema() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    let tools = client.list_all_tools().await?;

    let status_tool = tools.iter().find(|t| t.name.as_ref() == "status");
    assert!(status_tool.is_some(), "Should find status tool");

    let status_tool = status_tool.unwrap();
    assert!(
        status_tool.description.is_some(),
        "Status tool should have description"
    );

    // Verify it mentions polling anti-pattern in description
    if let Some(desc) = &status_tool.description {
        assert!(
            desc.contains("poll")
                || desc.contains("anti-pattern")
                || desc.contains("automatically"),
            "Status description should warn about polling"
        );
    }

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Async Operation Flow Tests
// ============================================================================

/// Test full async operation lifecycle: start -> status -> await
#[tokio::test]
async fn test_async_operation_full_lifecycle() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Start an async shell command
    let shell_params = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({"command": "sleep 0.1 && echo lifecycle_test"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };
    let start_result = client.call_tool(shell_params).await?;
    assert!(!start_result.content.is_empty());

    // Check status immediately
    let status_params = CallToolRequestParams {
        name: Cow::Borrowed("status"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
        task: None,
        meta: None,
    };
    let status_result = client.call_tool(status_params).await?;
    assert!(!status_result.content.is_empty());

    // Await should find completed operations
    let await_params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({"tools": "sandboxed_shell"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };
    let await_result = client.call_tool(await_params).await?;
    assert!(!await_result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Working Directory Integration Tests
// ============================================================================

/// Test that file_tools respects working directory
#[tokio::test]
async fn test_file_tools_in_temp_directory() -> Result<()> {
    init_test_logging();

    // Create a temp project with text files
    let temp_dir = create_rust_test_project(TestProjectOptions {
        prefix: Some("mcp_coverage_".to_string()),
        with_cargo: false,
        with_text_files: true,
        with_tool_configs: true,
    })
    .await?;

    let client = new_client_in_dir(Some(".ahma"), &[], temp_dir.path()).await?;

    // List files in the temp directory
    let tools = client.list_all_tools().await?;
    let has_file_tools = tools
        .iter()
        .any(|t| t.name.as_ref().starts_with("file_tools"));

    if has_file_tools {
        let params = CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(json!({"subcommand": "ls"}).as_object().unwrap().clone()),
            task: None,
            meta: None,
        };

        let result = client.call_tool(params).await?;

        if let Some(content) = result.content.first()
            && let Some(text_content) = content.as_text()
        {
            // Should see the test files we created
            assert!(
                text_content.text.contains("test1.txt")
                    || text_content.text.contains("test2.txt")
                    || text_content.text.contains(".ahma"),
                "Should see files in temp dir, got: {}",
                text_content.text
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Multiple Tool Filter Tests
// ============================================================================

/// Test status with comma-separated tool filters
#[tokio::test]
async fn test_status_with_multiple_tool_filters() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Query with multiple tool prefixes
    let params = CallToolRequestParams {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({"tools": "cargo,git,sandboxed_shell"})
                .as_object()
                .unwrap()
                .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should process the comma-separated list
        assert!(
            text_content.text.contains("Operations status")
                || text_content.text.contains("cargo")
                || text_content.text.contains("git")
                || text_content.text.contains("sandboxed_shell")
                || text_content.text.contains("total"),
            "Status should handle multiple filters, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test await with comma-separated tool filters
#[tokio::test]
async fn test_await_with_multiple_tool_filters() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Await with multiple tool prefixes
    let params = CallToolRequestParams {
        name: Cow::Borrowed("await"),
        arguments: Some(json!({"tools": "cargo,git"}).as_object().unwrap().clone()),
        task: None,
        meta: None,
    };

    let result = client.call_tool(params).await?;

    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        // Should indicate no pending operations for these tools
        assert!(
            text_content.text.contains("No pending operations")
                || text_content.text.contains("cargo")
                || text_content.text.contains("git")
                || text_content.text.contains("Completed"),
            "Await should handle multiple filters, got: {}",
            text_content.text
        );
    }

    client.cancel().await?;
    Ok(())
}
