//! Expanded path security edge case tests (See agent-plan.md Phase A)
use ahma_core::test_utils as common;
use ahma_core::utils::logging::init_test_logging;
use common::get_workspace_path;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::{fs, path::Path};

#[tokio::test]
async fn test_path_validation_nested_parent_segments() {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await.unwrap();
    // Deep relative escape attempt
    let params = CallToolRequestParam {
        name: "sandboxed_shell".into(),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "echo test",
                "working_directory": "a/b/c/../../../../"
            }))
            .unwrap(),
        ),
    };
    let result = client.call_tool(params).await;
    assert!(
        result.is_err(),
        "Nested parent segments escaping root should be rejected"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_path_validation_unicode_directory() {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await.unwrap();
    // Create a unicode directory inside workspace
    let unicode_dir = get_workspace_path("测试目录");
    let _ = fs::create_dir_all(&unicode_dir); // ignore if exists
    let rel = unicode_dir
        .strip_prefix(get_workspace_path("."))
        .unwrap_or(&unicode_dir);
    let rel_str = rel.to_string_lossy();
    let params = CallToolRequestParam {
        name: "sandboxed_shell".into(),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "echo unicode",
                "working_directory": rel_str
            }))
            .unwrap(),
        ),
    };
    let result = client.call_tool(params).await;
    assert!(
        result.is_ok(),
        "Unicode directory within workspace should be accepted"
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_path_validation_symlink_escape() {
    init_test_logging();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let client = new_client(Some(".ahma/tools")).await.unwrap();
        // Create symlink inside workspace pointing outside (e.g. /etc)
        let link_path = get_workspace_path("escape_link");
        // If link exists from prior run remove and recreate
        let _ = fs::remove_file(&link_path);
        symlink(Path::new("/etc"), &link_path).unwrap();
        let rel = link_path
            .strip_prefix(get_workspace_path("."))
            .unwrap_or(&link_path);
        let params = CallToolRequestParam {
            name: "sandboxed_shell".into(),
            arguments: Some(
                serde_json::from_value(json!({
                    "command": "echo test",
                    "working_directory": rel.to_string_lossy()
                }))
                .unwrap(),
            ),
        };
        let result = client.call_tool(params).await;
        assert!(result.is_err(), "Symlink escaping root should be rejected");
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn test_path_validation_symlink_internal() {
    init_test_logging();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let client = new_client(Some(".ahma/tools")).await.unwrap();
        // Create a directory and symlink pointing to it inside workspace
        let target_dir = get_workspace_path("internal_target");
        let _ = fs::create_dir_all(&target_dir);
        let link_path = get_workspace_path("internal_link");
        let _ = fs::remove_file(&link_path);
        symlink(&target_dir, &link_path).unwrap();
        let rel = link_path
            .strip_prefix(get_workspace_path("."))
            .unwrap_or(&link_path);
        let params = CallToolRequestParam {
            name: "sandboxed_shell".into(),
            arguments: Some(
                serde_json::from_value(json!({
                    "command": "echo ok",
                    "working_directory": rel.to_string_lossy()
                }))
                .unwrap(),
            ),
        };
        let result = client.call_tool(params).await;
        assert!(result.is_ok(), "Internal symlink should be accepted");
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn test_path_validation_reserved_names() {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await.unwrap();
    for wd in [".", "./", "././."] {
        let params = CallToolRequestParam {
            name: "sandboxed_shell".into(),
            arguments: Some(
                serde_json::from_value(json!({
                    "command": "echo here",
                    "working_directory": wd
                }))
                .unwrap(),
            ),
        };
        let result = client.call_tool(params).await;
        assert!(
            result.is_ok(),
            "Reserved current directory patterns should be accepted: {wd}"
        );
    }
    client.cancel().await.unwrap();
}
