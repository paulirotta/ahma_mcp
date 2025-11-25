//! Expanded path security edge case tests (See agent-plan.md Phase A)
use ahma_core::test_utils as common;
use ahma_core::utils::logging::init_test_logging;
use common::test_client::{get_workspace_tools_dir, new_client_in_dir};
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::{fs, path::Path};

#[tokio::test]
async fn test_path_validation_nested_parent_segments() {
    init_test_logging();
    let temp_dir = tempfile::tempdir().unwrap();
    let tools_dir = get_workspace_tools_dir();
    let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path())
        .await
        .unwrap();
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
    let temp_dir = tempfile::tempdir().unwrap();
    let tools_dir = get_workspace_tools_dir();
    let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path())
        .await
        .unwrap();
    // Create a unicode directory inside workspace
    let unicode_dir = temp_dir.path().join("test_dir_unicode");
    let _ = fs::create_dir_all(&unicode_dir); // ignore if exists
    let rel = unicode_dir
        .strip_prefix(temp_dir.path())
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
        let temp_dir = tempfile::tempdir().unwrap();
        let tools_dir = get_workspace_tools_dir();
        let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path())
            .await
            .unwrap();
        // Create symlink inside workspace pointing outside (e.g. /etc)
        let link_path = temp_dir.path().join("escape_link");
        // If link exists from prior run remove and recreate
        let _ = fs::remove_file(&link_path);
        symlink(Path::new("/etc"), &link_path).unwrap();
        let rel = link_path
            .strip_prefix(temp_dir.path())
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
        let temp_dir = tempfile::tempdir().unwrap();
        let tools_dir = get_workspace_tools_dir();
        let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path())
            .await
            .unwrap();
        // Create a directory and symlink pointing to it inside workspace
        let target_dir = temp_dir.path().join("internal_target");
        let _ = fs::create_dir_all(&target_dir);
        let link_path = temp_dir.path().join("internal_link");
        let _ = fs::remove_file(&link_path);
        symlink(&target_dir, &link_path).unwrap();
        let rel = link_path
            .strip_prefix(temp_dir.path())
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
    let temp_dir = tempfile::tempdir().unwrap();
    let tools_dir = get_workspace_tools_dir();
    let client = new_client_in_dir(Some(tools_dir.to_str().unwrap()), &[], temp_dir.path())
        .await
        .unwrap();
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
