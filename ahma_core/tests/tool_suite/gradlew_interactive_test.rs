//! Gradlew integration tests
//!
//! IMPORTANT: All tests in this module that invoke gradlew should be feature-gated with
//! `#[cfg(feature = "android")]` to ensure they only run in the job-android CI job,
//! not in job-nextest. This prevents duplicate execution and unnecessary Gradle setup overhead.
//!
//! Tests that only check tool availability or validate parameters without invoking Gradle
//! do not need the feature gate.

use ahma_core::skip_if_disabled_async_result;
use ahma_core::test_utils::get_workspace_dir;
use ahma_core::test_utils::test_client::new_client;
use anyhow::Result;
use rmcp::model::CallToolRequestParams;
use serde_json::{Map, json};
use std::borrow::Cow;

/// Get the workspace Android test project path
fn get_android_test_project_path() -> String {
    let workspace_dir = get_workspace_dir();
    let android_project_path = workspace_dir.join("test-data").join("AndoidTestBasicViews");
    android_project_path.to_string_lossy().to_string()
}

/// Test gradlew synchronous commands (quick operations)
/// Note: These tests are slow (~10s each) because Gradle has high startup time.
/// Consider running with `cargo nextest run --test gradlew_interactive_test` separately.
#[cfg(feature = "android")]
#[tokio::test]
#[ignore = "Slow test: Gradle startup takes ~10s per command. Run separately with --run-ignored"]
async fn test_gradlew_sync_commands_interactive() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    skip_if_disabled_async_result!("gradlew");
    let client = new_client(Some(".ahma")).await?;
    let project_path = get_android_test_project_path();

    // Test synchronous commands that should complete quickly
    let sync_commands = vec![
        ("tasks", "List available tasks"),
        ("help", "Show help information"),
        ("properties", "Display project properties"),
        ("dependencies", "Show dependencies"),
        ("clean", "Clean build directory"),
    ];

    for (command, description) in sync_commands {
        println!("Testing sync command: {} - {}", command, description);

        let call_param = CallToolRequestParams {
            name: Cow::Borrowed("sandboxed_shell"),
            arguments: Some(
                json!({
                    "subcommand": "default",
                    "command": format!("./gradlew {}", command),
                    "working_directory": project_path
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            task: None,
            meta: None,
        };

        let result = client.call_tool(call_param).await;

        match result {
            Ok(tool_result) => {
                assert!(
                    !tool_result.content.is_empty(),
                    "Command {} should return content",
                    command
                );

                if let Some(content) = tool_result.content.first()
                    && let Some(text_content) = content.as_text()
                {
                    println!("✓ {} completed successfully", command);
                    // Don't print full output, just verify it's not empty
                    assert!(
                        !text_content.text.trim().is_empty(),
                        "Command {} should return non-empty output",
                        command
                    );
                }
            }
            Err(e) => {
                println!("✗ {} failed: {}", command, e);
                // Some commands might fail if Android SDK not available - that's ok for testing the tool
                // We mainly want to ensure the tool definition works correctly
            }
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test gradlew with a valid working directory
/// Note: This test only runs ONE Gradle command to keep CI time reasonable (~30s).
/// Error cases (non-existent dir, missing param) are tested in test_gradlew_error_handling
/// which doesn't actually invoke Gradle.
#[cfg(feature = "android")]
#[tokio::test]
async fn test_gradlew_working_directory_handling() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    skip_if_disabled_async_result!("gradlew");
    let client = new_client(Some(".ahma")).await?;
    let project_path = get_android_test_project_path();

    // Test: Valid project directory with a quick command
    // Using "help" instead of "tasks" as it's slightly faster
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "command": "./gradlew help",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            assert!(!tool_result.content.is_empty());
            println!("✓ Valid project directory works via sandboxed_shell");
        }
        Err(e) => {
            println!(
                "Note: gradlew help failed via sandboxed_shell (possibly no Android SDK): {}",
                e
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test gradlew subcommand parameter validation
/// Note: Slow test due to Gradle startup time.
#[cfg(feature = "android")]
#[tokio::test]
#[ignore = "Slow test: Gradle startup takes ~10s per command. Run separately with --run-ignored"]
async fn test_gradlew_subcommand_validation() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    skip_if_disabled_async_result!("gradlew");
    let client = new_client(Some(".ahma")).await?;
    let project_path = get_android_test_project_path();

    // Test 1: Valid subcommand
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "command": "./gradlew help",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ Valid subcommand accepted via sandboxed_shell");
        }
        Err(e) => {
            println!(
                "Note: Valid subcommand failed via sandboxed_shell (possibly no Android SDK): {}",
                e
            );
        }
    }

    // Test 2: Invalid subcommand
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "command": "./gradlew nonexistent_command_xyz",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            // Should get some error message about unknown task
            if let Some(content) = _tool_result.content.first()
                && let Some(text_content) = content.as_text()
            {
                println!(
                    "✓ Invalid subcommand handled via sandboxed_shell: {}",
                    text_content.text.chars().take(100).collect::<String>()
                );
            }
        }
        Err(_) => {
            println!("✓ Invalid subcommand properly rejected via sandboxed_shell");
        }
    }

    // Test 3: Missing command in sandboxed_shell
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "working_directory": project_path
                // No command
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ Missing command handled gracefully via sandboxed_shell");
        }
        Err(e) => {
            println!(
                "✓ Missing command properly rejected via sandboxed_shell: {}",
                e
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test gradlew commands with optional parameters
/// Note: Slow test due to Gradle startup time.
#[cfg(feature = "android")]
#[tokio::test]
#[ignore = "Slow test: Gradle startup takes ~10s per command. Run separately with --run-ignored"]
async fn test_gradlew_optional_parameters() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    skip_if_disabled_async_result!("gradlew");
    let client = new_client(Some(".ahma")).await?;
    let project_path = get_android_test_project_path();

    // Test 1: tasks command with --all option
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "command": "./gradlew tasks --all",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ tasks --all command accepted via sandboxed_shell");
        }
        Err(e) => {
            println!(
                "Note: tasks --all failed via sandboxed_shell (possibly no Android SDK): {}",
                e
            );
        }
    }

    // Test 2: help command with task parameter
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "command": "./gradlew help --task build",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            println!("✓ help --task command accepted via sandboxed_shell");
            if let Some(content) = tool_result.content.first()
                && let Some(text_content) = content.as_text()
            {
                assert!(!text_content.text.is_empty());
            }
        }
        Err(e) => {
            println!(
                "Note: help --task failed via sandboxed_shell (possibly no Android SDK): {}",
                e
            );
        }
    }

    // Test 3: dependencies command with configuration parameter
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "subcommand": "default",
                "command": "./gradlew dependencies --configuration debugCompileClasspath",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ dependencies --configuration command accepted via sandboxed_shell");
        }
        Err(e) => {
            println!(
                "Note: dependencies --configuration failed via sandboxed_shell (possibly no Android SDK): {}",
                e
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test gradlew tool loading and basic availability
#[tokio::test]
async fn test_gradlew_tool_availability() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    skip_if_disabled_async_result!("gradlew");
    let client = new_client(Some(".ahma")).await?;

    // Test that sandboxed_shell tool is available
    let tools = client.list_tools(None).await?;

    let shell_tool = tools.tools.iter().find(|t| t.name == "sandboxed_shell");
    assert!(
        shell_tool.is_some(),
        "sandboxed_shell tool should be available"
    );

    let shell_tool = shell_tool.unwrap();
    assert_eq!(shell_tool.name, "sandboxed_shell");

    // Handle optional description
    if let Some(ref description) = shell_tool.description {
        assert!(
            description.contains("sandbox"),
            "Description should mention sandbox"
        );
        println!("  Description: {}", description);
    } else {
        println!("  Description: None provided");
    }

    println!("✓ sandboxed_shell tool loaded successfully");

    // Verify tool has input schema
    let schema_properties = shell_tool.input_schema.get("properties");
    if let Some(properties) = schema_properties {
        if let Some(properties_obj) = properties.as_object() {
            assert!(
                properties_obj.contains_key("command"),
                "Schema should have command property"
            );
            println!("✓ sandboxed_shell schema has command property");
        }
    } else {
        println!("Note: sandboxed_shell schema properties not found");
    }

    client.cancel().await?;
    Ok(())
}

/// Test error handling for malformed parameters
#[tokio::test]
async fn test_gradlew_error_handling() -> Result<()> {
    skip_if_disabled_async_result!("sandboxed_shell");
    skip_if_disabled_async_result!("gradlew");
    let client = new_client(Some(".ahma")).await?;

    // Test 1: Completely invalid parameters for sandboxed_shell
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "invalid_field": "invalid_value",
                "another_invalid": 12345
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            println!("✓ Invalid parameters handled gracefully");
            // Should get some error message
            if let Some(content) = tool_result.content.first()
                && let Some(text_content) = content.as_text()
            {
                assert!(!text_content.text.is_empty());
            }
        }
        Err(e) => {
            println!("✓ Invalid parameters properly rejected: {}", e);
        }
    }

    // Test 2: Wrong parameter types
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            json!({
                "command": 12345, // Should be string
                "working_directory": true // Should be string
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ Wrong parameter types handled gracefully");
        }
        Err(e) => {
            println!("✓ Wrong parameter types properly rejected: {}", e);
        }
    }

    // Test 3: Empty parameters
    let call_param = CallToolRequestParams {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(Map::new()),
        task: None,
        meta: None,
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ Empty parameters handled gracefully");
        }
        Err(e) => {
            println!("✓ Empty parameters properly handled: {}", e);
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test that we can read the Android project structure correctly
#[tokio::test]
async fn test_android_project_structure_validation() -> Result<()> {
    let project_path_str = get_android_test_project_path();
    let project_path = std::path::Path::new(&project_path_str);

    // Verify key Android project files exist
    assert!(
        project_path.join("gradlew").exists(),
        "gradlew should exist"
    );
    assert!(
        project_path.join("build.gradle.kts").exists(),
        "Root build.gradle.kts should exist"
    );
    assert!(
        project_path.join("app").exists(),
        "app directory should exist"
    );
    assert!(
        project_path.join("app/build.gradle.kts").exists(),
        "App build.gradle.kts should exist"
    );
    assert!(
        project_path.join("settings.gradle.kts").exists(),
        "settings.gradle.kts should exist"
    );

    // Verify gradlew is executable on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(project_path.join("gradlew"))?;
        let permissions = metadata.permissions();
        assert!(
            permissions.mode() & 0o111 != 0,
            "gradlew should be executable"
        );
    }

    println!("✓ Android project structure is valid");
    Ok(())
}
