//! Tests for path security and sandboxing
mod common;

use common::test_client::new_client;
use rmcp::{
    ServiceError,
    model::{CallToolRequestParam, ErrorCode},
};
use serde_json::json;
use tempfile::tempdir;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn test_path_validation_success() {
    let dir = tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    fs::create_dir(&tools_dir).await.unwrap();

    let ls_config = r#"{
        "name": "ls",
        "description": "list files",
        "command": "ls",
        "subcommand": [{
            "name": "default",
            "description": "list",
            "options": [{
                "name": "path",
                "type": "string",
                "format": "path",
                "description": "path to list"
            }]
        }]
    }"#;
    let mut file = File::create(tools_dir.join("ls.json")).await.unwrap();
    file.write_all(ls_config.as_bytes()).await.unwrap();

    let client = new_client(Some(tools_dir.to_str().unwrap())).await.unwrap();

    let params = CallToolRequestParam {
        name: "ls_default".into(),
        arguments: Some(serde_json::from_value(json!({ "path": "." })).unwrap()),
    };

    let result = client.call_tool(params).await;
    assert!(result.is_ok());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_path_validation_failure_absolute() {
    let dir = tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    fs::create_dir(&tools_dir).await.unwrap();

    let ls_config = r#"{
        "name": "ls",
        "description": "list files",
        "command": "ls",
        "subcommand": [{
            "name": "default",
            "description": "list",
            "options": [{
                "name": "path",
                "type": "string",
                "format": "path",
                "description": "path to list"
            }]
        }]
    }"#;
    let mut file = File::create(tools_dir.join("ls.json")).await.unwrap();
    file.write_all(ls_config.as_bytes()).await.unwrap();

    let client = new_client(Some(tools_dir.to_str().unwrap())).await.unwrap();

    let params = CallToolRequestParam {
        name: "ls_default".into(),
        arguments: Some(serde_json::from_value(json!({ "path": "/etc" })).unwrap()),
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
    let dir = tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    fs::create_dir(&tools_dir).await.unwrap();

    let ls_config = r#"{
        "name": "ls",
        "description": "list files",
        "command": "ls",
        "subcommand": [{
            "name": "default",
            "description": "list",
            "options": [{
                "name": "path",
                "type": "string",
                "format": "path",
                "description": "path to list"
            }]
        }]
    }"#;
    let mut file = File::create(tools_dir.join("ls.json")).await.unwrap();
    file.write_all(ls_config.as_bytes()).await.unwrap();

    let client = new_client(Some(tools_dir.to_str().unwrap())).await.unwrap();

    let params = CallToolRequestParam {
        name: "ls_default".into(),
        arguments: Some(serde_json::from_value(json!({ "path": "../" })).unwrap()),
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
