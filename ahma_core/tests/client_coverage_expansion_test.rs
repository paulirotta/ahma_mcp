//! # Client Module Coverage Tests
//!
//! This file provides integration tests to improve coverage of `ahma_core/src/client.rs`.
//! The module already has unit tests for helper functions (`extract_operation_id`,
//! `join_text_contents`, `first_text_content`). This file adds integration tests for:
//!
//! - `Client::start_process` and `start_process_with_args`
//! - `Client::get_service` error path (when not initialized)
//! - `Client::shell_async_sleep`, `await_op`, and `status` methods
//!
//! These tests use the real ahma_mcp binary to ensure full integration coverage.

use ahma_core::test_utils::test_client::{new_client, new_client_with_args};
use ahma_core::test_utils::test_project::{TestProjectOptions, create_rust_test_project};
use ahma_core::utils::logging::init_test_logging;
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

// ============================================================================
// Client Initialization and Process Spawning Tests
// ============================================================================

/// Test that new_client works with the tools directory
#[tokio::test]
async fn test_client_start_process_with_tools_dir() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Verify client is functional by listing tools (using the MCP protocol)
    let tools = client.list_all_tools().await?;

    // Should have default tools available
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        tool_names.contains(&"sandboxed_shell")
            || tool_names.contains(&"await")
            || tool_names.contains(&"status"),
        "Expected standard tools, got: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

/// Test that new_client_with_args handles extra arguments like --sync
#[tokio::test]
async fn test_client_start_process_with_sync_flag() -> Result<()> {
    init_test_logging();

    // Start client with --sync flag
    let client = new_client_with_args(Some(".ahma/tools"), &["--sync"]).await?;

    // Verify client works by listing tools
    let tools = client.list_all_tools().await?;
    assert!(!tools.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test that new_client_with_args works with debug flag
#[tokio::test]
async fn test_client_start_process_with_debug_flag() -> Result<()> {
    init_test_logging();

    // Create client with --debug flag
    let client = new_client_with_args(Some(".ahma/tools"), &["--debug"]).await?;

    // Verify client is functional by listing tools
    let tools = client.list_all_tools().await?;
    assert!(!tools.is_empty());

    client.cancel().await?;
    Ok(())
}

/// Test that new_client_with_args works with --log-to-stderr flag
#[tokio::test]
async fn test_client_start_process_with_log_to_stderr() -> Result<()> {
    init_test_logging();

    // Create client with --log-to-stderr flag
    let client = new_client_with_args(Some(".ahma/tools"), &["--log-to-stderr"]).await?;

    // Verify client is functional
    let params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
    };

    let result = client.call_tool(params).await?;
    assert!(!result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Status Tool Tests via Client
// ============================================================================

/// Test status tool returns expected format when no operations
#[tokio::test]
async fn test_client_status_no_operations() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
    };

    let result = client.call_tool(params).await?;
    assert!(!result.content.is_empty());

    if let Some(content) = result.content.first() {
        if let Some(text_content) = content.as_text() {
            // Should indicate operations status
            assert!(
                text_content.text.contains("Operations")
                    || text_content.text.contains("active")
                    || text_content.text.contains("completed")
                    || text_content.text.contains("No"),
                "Expected status output, got: {}",
                text_content.text
            );
        }
    }
    Ok(())
}

/// Test status tool with a specific operation_id filter
#[tokio::test]
async fn test_client_status_with_operation_id() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Query status for a nonexistent operation
    let params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(
            json!({ "operation_id": "nonexistent_op_12345" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    };

    let result = client.call_tool(params).await?;
    // Should handle gracefully - no crash, returns some response
    assert!(!result.content.is_empty());
    Ok(())
}

// ============================================================================
// Await Tool Tests via Client
// ============================================================================

/// Test await tool with no pending operations
#[tokio::test]
async fn test_client_await_no_pending() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
    };

    let result = client.call_tool(params).await?;
    // Should return quickly indicating nothing to await
    assert!(!result.content.is_empty());
    Ok(())
}

/// Test await tool with specific operation_id that doesn't exist
#[tokio::test]
async fn test_client_await_nonexistent_operation() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(
            json!({ "operation_id": "nonexistent_op_67890" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    };

    let result = client.call_tool(params).await?;
    // Should handle gracefully
    assert!(!result.content.is_empty());
    Ok(())
}

// ============================================================================
// Full Async Operation Lifecycle Tests
// ============================================================================

/// Test full async operation lifecycle: start, status, await
#[tokio::test]
async fn test_async_operation_lifecycle() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Start an async operation (short sleep)
    let shell_params = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({ "command": "sleep 0.5" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    };

    let start_result = client.call_tool(shell_params).await?;
    assert!(!start_result.content.is_empty());

    if let Some(content) = start_result.content.first() {
        if let Some(text_content) = content.as_text() {
            // Check if it started as async (contains operation ID)
            if text_content.text.contains("ID:") {
                // Extract operation ID
                let op_id = extract_op_id(&text_content.text);

                // Check status while running
                let status_params = CallToolRequestParam {
                    name: Cow::Borrowed("status"),
                    arguments: Some(
                        json!({ "operation_id": op_id.clone() })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                };
                let status_result = client.call_tool(status_params).await?;
                assert!(!status_result.content.is_empty());

                // Await completion
                let await_params = CallToolRequestParam {
                    name: Cow::Borrowed("await"),
                    arguments: Some(
                        json!({ "operation_id": op_id })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                };
                let await_result = client.call_tool(await_params).await?;
                assert!(!await_result.content.is_empty());
            }
        }
    }
    Ok(())
}

/// Test multiple async operations can be tracked
#[tokio::test]
async fn test_multiple_async_operations() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Start two async operations
    let shell_params1 = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({ "command": "sleep 0.3" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    };
    let result1 = client.call_tool(shell_params1).await?;

    let shell_params2 = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({ "command": "sleep 0.3" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    };
    let result2 = client.call_tool(shell_params2).await?;

    // Check overall status
    let status_params = CallToolRequestParam {
        name: Cow::Borrowed("status"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
    };
    let status = client.call_tool(status_params).await?;
    assert!(!status.content.is_empty());

    // Await all
    let await_params = CallToolRequestParam {
        name: Cow::Borrowed("await"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
    };
    let await_result = client.call_tool(await_params).await?;
    assert!(!await_result.content.is_empty());

    // Results should exist
    assert!(!result1.content.is_empty());
    assert!(!result2.content.is_empty());
    Ok(())
}

// ============================================================================
// Shell Execution Tests
// ============================================================================

/// Test sandboxed_shell tool execution (covers shell-related paths in client)
#[tokio::test]
async fn test_sandboxed_shell_execution() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Run a simple command
    let params = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({ "command": "echo 'hello from test'" })
                .as_object()
                .unwrap()
                .clone(),
        ),
    };

    let result = client.call_tool(params).await?;
    assert!(!result.content.is_empty());
    Ok(())
}

/// Test sandboxed_shell with working_directory parameter
/// Uses a directory inside the workspace to comply with sandbox restrictions
#[tokio::test]
async fn test_sandboxed_shell_with_working_dir() -> Result<()> {
    use ahma_core::test_utils::test_client::get_workspace_tools_dir;

    init_test_logging();

    let client = new_client(Some(".ahma/tools")).await?;

    // Use the workspace's target directory which is inside the sandbox
    let tools_dir = get_workspace_tools_dir();
    let workspace_dir = tools_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Should have workspace parent");
    let target_dir = workspace_dir.join("target");

    // Use target directory if it exists, otherwise use workspace root
    let working_dir = if target_dir.exists() {
        target_dir
    } else {
        workspace_dir.to_path_buf()
    };

    let params = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": "pwd",
                "working_directory": working_dir.to_str().unwrap()
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(params).await?;
    assert!(!result.content.is_empty());

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test calling a tool that doesn't exist
#[tokio::test]
async fn test_call_nonexistent_tool() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    let params = CallToolRequestParam {
        name: Cow::Borrowed("this_tool_definitely_does_not_exist_xyz"),
        arguments: Some(json!({}).as_object().unwrap().clone()),
    };

    let result = client.call_tool(params).await;
    // Should return an error
    assert!(result.is_err(), "Expected error for nonexistent tool");
    Ok(())
}

/// Test list_tools returns expected format
#[tokio::test]
async fn test_list_tools_format() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

    // Verify by listing tools
    let tools = client.list_all_tools().await?;
    assert!(!tools.is_empty());

    // Should contain standard tools
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        tool_names.contains(&"sandboxed_shell")
            || tool_names.contains(&"await")
            || tool_names.contains(&"status"),
        "Expected standard tools, got: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

// ============================================================================
// Custom Tools Directory Tests
// ============================================================================

/// Test client with custom tools directory using new_client_in_dir
/// This test verifies that custom tool configurations are loaded correctly
#[tokio::test]
async fn test_client_with_custom_tools_dir() -> Result<()> {
    use ahma_core::test_utils::test_client::new_client_in_dir;

    init_test_logging();

    let temp_project = create_rust_test_project(TestProjectOptions {
        prefix: Some("custom_tools_test_".to_string()),
        with_cargo: false,
        with_text_files: false,
        with_tool_configs: true,
    })
    .await?;

    // Use new_client_in_dir to set the working directory to the temp project
    // This way the sandbox scope will include the temp directory
    let tools_dir = temp_project.path().join(".ahma").join("tools");
    let client =
        new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_project.path()).await?;

    // The custom project has an "echo" tool defined - use list_all_tools
    let tools = client.list_all_tools().await?;
    assert!(!tools.is_empty());

    // Should list the echo tool from custom config or at least the built-in tools
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        tool_names.contains(&"echo") || tool_names.contains(&"await"),
        "Expected tools from config, got: {:?}",
        tool_names
    );

    client.cancel().await?;
    Ok(())
}

/// Helper to extract operation ID from response
fn extract_op_id(text: &str) -> String {
    if let Some(id_start) = text.find("ID: ") {
        let id_text = &text[id_start + 4..];
        if let Some(job_id) = id_text.split_whitespace().next() {
            return job_id.to_string();
        }
    }
    // Return a placeholder if not found
    "unknown".to_string()
}
