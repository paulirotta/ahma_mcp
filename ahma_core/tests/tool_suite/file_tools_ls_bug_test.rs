/// Tests for file_tools ls command bug
///
/// This test reproduces the reported issue:
/// Error: MCP -32603 with file_tools ls command showing
/// 'ls: --long: No such file or directory' even when directory exists
///
/// The root cause appears to be incorrect handling of boolean flags
/// where "--long" is being passed as a path argument instead of being
/// converted to the "-l" flag format.
use ahma_core::skip_if_disabled_async;
use ahma_core::test_utils as common;

use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

#[tokio::test]
async fn test_ls_command_with_long_flag() {
    skip_if_disabled_async!("file_tools");
    // Setup: Create a test client with file_tools
    let client = new_client(Some(".ahma/tools"))
        .await
        .expect("Failed to create test client");

    // Test: Call ls with long flag on current directory (which definitely exists)
    let args = json!({
        "subcommand": "ls",
        "path": ".",
        "long": "true"
    });

    let result = client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
        })
        .await;

    // Assert: Should succeed, not error with "ls: --long: No such file or directory"
    assert!(
        result.is_ok(),
        "ls command with long flag should succeed, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should not contain the error message about --long being treated as a path
    assert!(
        !output.contains("--long: No such file or directory"),
        "Output should not contain error about --long flag: {}",
        output
    );

    // Should not contain the error message about flag not found
    assert!(
        !output.contains("ls: unrecognized option"),
        "Output should not contain unrecognized option error: {}",
        output
    );

    // Should contain actual directory listing output
    // At minimum, should list the Cargo.toml file in current directory
    assert!(
        output.contains("Cargo.toml") || output.contains("total"),
        "Output should contain directory listing, got: {}",
        output
    );
}

#[tokio::test]
async fn test_ls_command_without_flags() {
    skip_if_disabled_async!("file_tools");
    // Setup
    let client = new_client(Some(".ahma/tools"))
        .await
        .expect("Failed to create test client");

    // Test: Call ls without any flags
    let args = json!({
        "subcommand": "ls",
        "path": "."
    });

    let result = client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
        })
        .await;

    // Assert: Should succeed
    assert!(
        result.is_ok(),
        "ls command should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should list files
    assert!(
        output.contains("Cargo.toml"),
        "Output should contain directory listing, got: {}",
        output
    );
}

#[tokio::test]
async fn test_ls_command_with_all_flag() {
    skip_if_disabled_async!("file_tools");
    // Setup
    let client = new_client(Some(".ahma/tools"))
        .await
        .expect("Failed to create test client");

    // Test: Call ls with --all flag
    let args = json!({
        "subcommand": "ls",
        "path": ".",
        "all": "true"
    });

    let result = client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
        })
        .await;

    // Assert: Should succeed
    assert!(
        result.is_ok(),
        "ls with all flag should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should not have flag-as-path error
    assert!(
        !output.contains("--all: No such file or directory"),
        "Output should not contain error about --all flag: {}",
        output
    );
}

#[tokio::test]
async fn test_ls_command_with_multiple_flags() {
    skip_if_disabled_async!("file_tools");
    // Setup
    let client = new_client(Some(".ahma/tools"))
        .await
        .expect("Failed to create test client");

    // Test: Call ls with multiple flags
    let args = json!({
        "subcommand": "ls",
        "path": ".",
        "long": "true",
        "all": "true",
        "human-readable": "true"
    });

    let result = client
        .call_tool(CallToolRequestParam {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
        })
        .await;

    // Assert: Should succeed with combined flags
    assert!(
        result.is_ok(),
        "ls with multiple flags should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should not have any flag-as-path errors
    assert!(
        !output.contains("No such file or directory"),
        "Output should not contain file not found errors for flags: {}",
        output
    );
}
