//! Tests for path security and sandboxing
mod common;

use common::test_client::new_client;
use rmcp::{
    ServiceError,
    model::{CallToolRequestParam, ErrorCode},
};
use serde_json::json;

#[tokio::test]
async fn test_path_validation_success() {
    // Use existing shell_async tool for path validation test
    let client = new_client(None).await.unwrap();

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

#[tokio::test]
async fn test_path_validation_failure_absolute() {
    // Use existing shell_async tool for path validation test
    let client = new_client(None).await.unwrap();

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
            assert!(
                mcp_error
                    .message
                    .contains("is outside the allowed workspace")
            );
        }
        _ => panic!("Expected McpError, got {:?}", error),
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_path_validation_failure_relative() {
    // Use existing shell_async tool for path validation test
    let client = new_client(None).await.unwrap();

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
            assert!(
                mcp_error
                    .message
                    .contains("is outside the allowed workspace")
            );
        }
        _ => panic!("Expected McpError, got {:?}", error),
    }
    client.cancel().await.unwrap();
}
