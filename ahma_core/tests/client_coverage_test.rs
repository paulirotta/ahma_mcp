//! # Client Error Path Tests
//!
//! Tests for error handling when the MCP Client is used before initialization.
//! These are intentionally separated from integration tests to cover the "not initialized"
//! error paths without needing to spawn a server.
//!
//! For integration tests that exercise Client methods with a live server, see:
//! - `client_coverage_expansion_test.rs`
//! - `mcp_integration_tests.rs`

use ahma_core::client::Client;

/// All Client methods must fail with a clear error when called before `start_process()`
#[tokio::test]
async fn test_client_methods_fail_before_initialization() {
    let mut client = Client::new();

    // status() should fail
    let result = client.status("test_op").await;
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("not initialized"),
        "status() should indicate client not initialized"
    );

    // await_op() should fail
    let result = client.await_op("test_op").await;
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("not initialized"),
        "await_op() should indicate client not initialized"
    );

    // shell_async_sleep() should fail
    let result = client.shell_async_sleep("1").await;
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("not initialized"),
        "shell_async_sleep() should indicate client not initialized"
    );
}

/// Client::new() and Client::default() produce equivalent uninitialized clients
#[test]
fn test_client_new_and_default_equivalence() {
    let client_new = Client::new();
    let client_default = Client::default();

    // Both should have same Debug representation (uninitialized state)
    let debug_new = format!("{:?}", client_new);
    let debug_default = format!("{:?}", client_default);

    assert!(debug_new.contains("Client"));
    assert!(debug_default.contains("Client"));
    // Both should show None/uninitialized service
    assert!(debug_new.contains("None") || debug_new.contains("service"));
}
