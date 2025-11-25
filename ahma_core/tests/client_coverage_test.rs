//! Test coverage for the client module
//!
//! These tests cover the Client struct and its methods for MCP testing.

use ahma_core::client::Client;

#[test]
fn test_client_creation() {
    // Test that Client::new() creates a valid client
    let _client = Client::new();
    // If we got here without panic, creation succeeded
}

#[test]
fn test_client_default_trait() {
    // Test that Client implements Default correctly
    let _client = Client::default();
    // If we got here without panic, Default works
}

#[tokio::test]
async fn test_client_get_service_before_init() {
    let mut client = Client::new();
    // Attempting to use methods before initialization should fail
    let result = client.status("test_op").await;
    assert!(
        result.is_err(),
        "Status should fail when client is not initialized"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not initialized"),
        "Error should indicate client not initialized, got: {}",
        err
    );
}

#[tokio::test]
async fn test_client_await_op_before_init() {
    let mut client = Client::new();
    let result = client.await_op("test_op").await;
    assert!(
        result.is_err(),
        "await_op should fail when client is not initialized"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not initialized"),
        "Error should indicate client not initialized, got: {}",
        err
    );
}

#[tokio::test]
async fn test_client_shell_async_sleep_before_init() {
    let mut client = Client::new();
    let result = client.shell_async_sleep("1").await;
    assert!(
        result.is_err(),
        "shell_async_sleep should fail when client is not initialized"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not initialized"),
        "Error should indicate client not initialized, got: {}",
        err
    );
}

#[test]
fn test_client_debug_trait() {
    let client = Client::new();
    let debug_output = format!("{:?}", client);
    assert!(
        debug_output.contains("Client"),
        "Debug output should contain struct name"
    );
}
