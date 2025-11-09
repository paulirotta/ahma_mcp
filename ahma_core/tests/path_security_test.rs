//! Tests for path security and sandboxing
mod common;

use ahma_core::utils::logging::init_test_logging;
use common::test_client::new_client;
use rmcp::{
    model::{CallToolRequestParam, ErrorCode},
    ServiceError,
};
use serde_json::json;

/// Path validation - validates that working_directory is within allowed workspace
#[tokio::test]
async fn test_path_validation_success() {
    init_test_logging();
    // Use existing shell_async tool for path validation test
    let client = new_client(Some(".ahma/tools")).await.unwrap();

    let params = CallToolRequestParam {
        name: "shell_async".into(),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "echo test",
                "working_directory": "."
            }))
            .unwrap(),
        ),
    };

    let result = client.call_tool(params).await;
    assert!(result.is_ok());
    client.cancel().await.unwrap();
}

/// Test path validation rejects absolute paths outside workspace
/// TODO: Implement path security validation in mcp_service.rs or adapter.rs
#[tokio::test]
#[ignore = "Feature not yet implemented - path security validation"]
async fn test_path_validation_failure_absolute() {
    init_test_logging();
    // Use existing shell_async tool for path validation test
    let client = new_client(Some(".ahma/tools")).await.unwrap();

    let params = CallToolRequestParam {
        name: "shell_async".into(),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "echo test",
                "working_directory": "/etc"
            }))
            .unwrap(),
        ),
    };

    let result = client.call_tool(params).await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    match error {
        ServiceError::McpError(mcp_error) => {
            assert_eq!(mcp_error.code, ErrorCode::INVALID_PARAMS);
            assert!(mcp_error
                .message
                .contains("is outside the allowed workspace"));
        }
        _ => panic!("Expected McpError, got {:?}", error),
    }
    client.cancel().await.unwrap();
}

/// Test path validation rejects relative paths that escape workspace
/// TODO: Implement path security validation in mcp_service.rs or adapter.rs
#[tokio::test]
#[ignore = "Feature not yet implemented - path security validation"]
async fn test_path_validation_failure_relative() {
    init_test_logging();
    // Use existing shell_async tool for path validation test
    let client = new_client(Some(".ahma/tools")).await.unwrap();

    let params = CallToolRequestParam {
        name: "shell_async".into(),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "echo test",
                "working_directory": "../"
            }))
            .unwrap(),
        ),
    };

    let result = client.call_tool(params).await;
    assert!(result.is_err());
    let error = result.unwrap_err();
    match error {
        ServiceError::McpError(mcp_error) => {
            assert_eq!(mcp_error.code, ErrorCode::INVALID_PARAMS);
            assert!(mcp_error
                .message
                .contains("is outside the allowed workspace"));
        }
        _ => panic!("Expected McpError, got {:?}", error),
    }
    client.cancel().await.unwrap();
}
