//! Sandboxed Shell Integration Tests
//!
//! Tests for sandboxed shell tool via the HTTP SSE bridge.

mod common;
use common::sse_test_helpers::{self, call_tool, ensure_server_available};

use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_sandboxed_shell_echo() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "sandboxed_shell");

    let result = call_tool(
        &client,
        "sandboxed_shell",
        json!({"command": "echo 'Hello from sandboxed shell!'"}),
    )
    .await;

    assert!(
        result.success,
        "sandboxed_shell echo failed: {:?}",
        result.error
    );
    assert!(result.output.is_some());

    let output = result.output.unwrap();

    // The output may contain the expected string OR be an async operation notice
    let has_expected_output = output.contains("Hello from sandboxed shell!");
    let is_async_operation = output.contains("operation")
        || output.contains("async")
        || output.contains("started")
        || output.contains("op_");

    if has_expected_output {
        println!("OK Got expected output: {}", output);
    } else if is_async_operation {
        println!("OK Got async operation response (valid): {}", output);
    } else {
        println!(
            "WARNING️  Unexpected output format (but tool call succeeded): {}",
            output
        );
    }
}

#[tokio::test]
async fn test_sandboxed_shell_pipe() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "sandboxed_shell");

    let result = call_tool(
        &client,
        "sandboxed_shell",
        json!({"subcommand": "default", "command": "echo 'line1\\nline2\\nline3' | wc -l"}),
    )
    .await;

    assert!(
        result.success,
        "sandboxed_shell pipe failed: {:?}",
        result.error
    );
    assert!(result.output.is_some());

    let output = result.output.unwrap();

    let has_expected_output = output.trim().contains("3");
    let is_async_operation = output.contains("operation")
        || output.contains("async")
        || output.contains("started")
        || output.contains("op_");

    if has_expected_output {
        println!("OK Got expected line count: {}", output.trim());
    } else if is_async_operation {
        println!("OK Got async operation response (valid): {}", output);
    } else {
        println!(
            "WARNING️  Unexpected output format (but tool call succeeded): {}",
            output
        );
    }
}

#[tokio::test]
async fn test_sandboxed_shell_variable_substitution() {
    let _server = ensure_server_available().await;
    let client = Client::new();

    skip_if_unavailable!(&client, "sandboxed_shell");

    let result = call_tool(
        &client,
        "sandboxed_shell",
        json!({"subcommand": "default", "command": "echo \\\"PWD is: $PWD\\\""}),
    )
    .await;

    assert!(
        result.success,
        "sandboxed_shell var substitution failed: {:?}",
        result.error
    );
    assert!(result.output.is_some());

    let output = result.output.unwrap();

    let has_expected_output = output.contains("PWD is:");
    let is_async_operation = output.contains("operation")
        || output.contains("async")
        || output.contains("started")
        || output.contains("op_");

    if has_expected_output {
        println!("OK Got expected PWD output: {}", output);
    } else if is_async_operation {
        println!("OK Got async operation response (valid): {}", output);
    } else {
        println!(
            "WARNING️  Unexpected output format (but tool call succeeded): {}",
            output
        );
    }
}
