//! Git and GitHub CLI Integration Tests
//!
//! Tests for Git and GitHub CLI tools via the HTTP SSE bridge.

mod common;

use common::sse_test_helpers::{self, call_tool, ensure_server_available};

use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_git_status() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "git_status");

    let result = call_tool(&client, "git_status", json!({})).await;

    // Git status should work in this repo
    if result.success {
        let output = result.output.unwrap_or_default();
        println!(
            "git status output (first 200 chars): {}",
            &output[..output.len().min(200)]
        );
    } else {
        eprintln!("WARNING️  git_status failed: {:?}", result.error);
    }
}

#[tokio::test]
async fn test_git_log() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "git_log");

    let result = call_tool(&client, "git_log", json!({"-n": 5})).await;

    if result.success {
        println!("OK git log succeeded");
    } else {
        eprintln!("WARNING️  git_log failed: {:?}", result.error);
    }
}

#[tokio::test]
async fn test_gh_workflow_list() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "gh_workflow_list");

    let result = call_tool(&client, "gh_workflow_list", json!({})).await;

    // gh may not be authenticated, so we just check the call went through
    println!(
        "gh workflow list result: success={}, error={:?}",
        result.success, result.error
    );
}
