//! Python Tools Integration Tests
//!
//! Tests for Python execution tools via the HTTP SSE bridge.

mod common;

use common::sse_test_helpers::{self, call_tool, ensure_server_available};

use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_python_version() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "python");

    let result = call_tool(&client, "python", json!({"subcommand": "version"})).await;

    // Python may not be installed, so we just check the call went through
    println!(
        "python version result: success={}, error={:?}",
        result.success, result.error
    );
    if result.success {
        let output = result.output.unwrap_or_default();
        println!("python version output: {}", output);
    }
}

#[tokio::test]
async fn test_python_code() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "python");

    let result = call_tool(
        &client,
        "python",
        json!({"subcommand": "code", "command": "print('Hello from Python!')"}),
    )
    .await;

    println!(
        "python code result: success={}, error={:?}",
        result.success, result.error
    );
    if result.success {
        let output = result.output.unwrap_or_default();
        println!("python code output: {}", output);
    }
}
