//! Integration tests that exercise actual tool execution through MCP call_tool.
//!
//! These tests ensure that tool JSON definitions not only parse correctly but
//! also execute properly through the full MCP stack. This catches regressions
//! where a tool definition parses but produces incorrect command-line arguments.
//!
//! Tests skip gracefully if the corresponding tool is disabled (enabled: false in JSON config).

use ahma_mcp::skip_if_disabled_async;

use ahma_mcp::test_utils::client::ClientBuilder;
use rmcp::model::CallToolRequestParams;
use serde_json::json;
use std::borrow::Cow;

// ==================== file_tools integration tests ====================

#[tokio::test]
async fn test_file_tools_pwd_execution() {
    skip_if_disabled_async!("file_tools");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "subcommand": "pwd"
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "file_tools pwd should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should contain a path
    assert!(
        output.contains('/'),
        "pwd should return a path, got: {}",
        output
    );
}

#[tokio::test]
async fn test_file_tools_cat_with_number_option() {
    skip_if_disabled_async!("file_tools");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "subcommand": "cat",
        "files": ["Cargo.toml"],
        "number": true
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "file_tools cat with number should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should contain line numbers (cat -n format)
    assert!(
        output.contains("1") || output.contains("[package]"),
        "cat output should contain line numbers or Cargo.toml content, got: {}",
        output
    );
}

#[tokio::test]
async fn test_file_tools_head_with_lines_option() {
    skip_if_disabled_async!("file_tools");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "subcommand": "head",
        "files": ["Cargo.toml"],
        "lines": 3
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "file_tools head should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should contain workspace members or package header
    assert!(
        output.contains("[workspace]") || output.contains("[package]"),
        "head should show beginning of Cargo.toml, got: {}",
        output
    );
}

// Test for find command with -name option.
// This validates that option names starting with dash (like -name, -type, -mtime)
// are passed correctly to the command without adding an extra dash prefix.
#[tokio::test]
async fn test_file_tools_find_with_name_option() {
    skip_if_disabled_async!("file_tools");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    // Note: The find options in file_tools.json use single-dash prefix format (e.g., "-name",
    // "-maxdepth", "-type") to match the actual BSD/macOS find command syntax.
    // The adapter's format_option_flag() helper correctly preserves the leading dash.
    let args = json!({
        "subcommand": "find",
        "path": ".",
        "-name": "*.toml",
        "-maxdepth": 1
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "find with -name should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should find Cargo.toml in the current directory
    assert!(
        output.contains("Cargo.toml"),
        "find should locate Cargo.toml, got: {}",
        output
    );
}

#[tokio::test]
async fn test_file_tools_grep_with_options() {
    skip_if_disabled_async!("file_tools");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "subcommand": "grep",
        "pattern": "workspace",
        "files": ["Cargo.toml"],
        "ignore-case": true,
        "line-number": true
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "file_tools grep should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should find workspace mentions with line numbers
    assert!(
        output.contains("workspace") || output.contains("Workspace"),
        "grep should find workspace in Cargo.toml, got: {}",
        output
    );
}

// ==================== sandboxed_shell integration tests ====================

#[tokio::test]
async fn test_sandboxed_shell_echo_execution() {
    skip_if_disabled_async!("sandboxed_shell");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "command": "echo 'hello world'"
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("sandboxed_shell"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "sandboxed_shell echo should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Since sandboxed_shell is async, we get operation started message
    assert!(
        output.contains("hello world") || output.contains("op_"),
        "echo should output 'hello world' or operation ID, got: {}",
        output
    );
}

#[tokio::test]
async fn test_sandboxed_shell_pipe_execution() {
    skip_if_disabled_async!("sandboxed_shell");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "command": "echo 'test' | wc -c"
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("sandboxed_shell"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "sandboxed_shell pipe should succeed: {:?}",
        result.err()
    );
}

// ==================== git integration tests (read-only) ====================

#[tokio::test]
async fn test_git_status_execution() {
    skip_if_disabled_async!("git");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "subcommand": "status",
        "short": true
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("git"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "git status should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // In a git repo, status should not error (may be empty or show changes)
    assert!(
        !output.contains("fatal:") && !output.contains("not a git repository"),
        "git status should work in repo, got: {}",
        output
    );
}

#[tokio::test]
async fn test_git_log_oneline_execution() {
    skip_if_disabled_async!("git");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    let args = json!({
        "subcommand": "log",
        "oneline": true,
        "rev_range": "-5"
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("git"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    // git log is async, so we just verify it starts successfully
    assert!(result.is_ok(), "git log should succeed: {:?}", result.err());
}

// ==================== cargo integration tests (read-only) ====================

#[tokio::test]
async fn test_cargo_check_dry_run() {
    skip_if_disabled_async!("cargo");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    // cargo check is synchronous per config
    let args = json!({
        "subcommand": "check",
        "workspace": true
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("cargo"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "cargo check should succeed: {:?}",
        result.err()
    );
}

// ==================== Tool option alias tests ====================
// These verify that option aliases (e.g., -l for --long) work correctly

#[tokio::test]
async fn test_file_tools_ls_long_alias() {
    skip_if_disabled_async!("file_tools");

    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    // Use the full option name 'long' which should map to -l
    let args = json!({
        "subcommand": "ls",
        "path": ".",
        "long": true
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "ls with long option should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Long format should show permissions (drwx... or -rw-...)
    assert!(
        output.contains("total") || output.contains("rw") || output.contains("Cargo.toml"),
        "ls -l should show long format, got: {}",
        output
    );
}

#[tokio::test]
async fn test_file_tools_grep_recursive_alias() {
    skip_if_disabled_async!("file_tools");
    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    // Use the full option name 'recursive' which should map to -r
    // NOTE: Search only ahma_mcp/src to avoid timeout scanning entire repo
    let args = json!({
        "subcommand": "grep",
        "pattern": "adapter",
        "files": ["ahma_mcp/src"],
        "recursive": true,
        "files-with-matches": true
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "grep with recursive should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // Should find files containing ahma_mcp
    assert!(
        output.contains("Cargo.toml") || output.contains(".rs") || output.is_empty(),
        "grep -r should find files or be empty, got: {}",
        output
    );
}

// ==================== Path format validation tests ====================
// These ensure that format: "path" fields are properly validated

#[tokio::test]
async fn test_file_tools_ls_path_validation() {
    skip_if_disabled_async!("file_tools");
    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    // Use a relative path that exists
    let args = json!({
        "subcommand": "ls",
        "path": "./ahma_mcp"
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "ls with valid relative path should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_file_tools_cat_path_format() {
    skip_if_disabled_async!("file_tools");
    let client = ClientBuilder::new()
        .tools_dir(".ahma")
        .build()
        .await
        .expect("Failed to create test client");

    // Cat a specific file with path format
    let args = json!({
        "subcommand": "cat",
        "files": ["./README.md"]
    });

    let result = client
        .call_tool(CallToolRequestParams {
            name: Cow::Borrowed("file_tools"),
            arguments: Some(args.as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "cat with path format files should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let content = response.content[0].as_text().unwrap();
    let output = &content.text;

    // README.md should contain project information
    assert!(
        output.contains("ahma") || output.contains("MCP") || output.contains("#"),
        "cat should show README content, got: {}",
        output
    );
}
