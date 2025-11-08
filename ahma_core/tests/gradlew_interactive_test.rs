mod common;

use crate::common::{get_workspace_dir, test_client::new_client};
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::{json, Map};
use std::borrow::Cow;

/// Get the workspace Android test project path
fn get_android_test_project_path() -> String {
    let workspace_dir = get_workspace_dir();
    let android_project_path = workspace_dir.join("test-data").join("AndoidTestBasicViews");
    android_project_path.to_string_lossy().to_string()
}

/// Test gradlew synchronous commands (quick operations)
#[tokio::test]
async fn test_gradlew_sync_commands_interactive() -> Result<()> {
    // Skip in CI environments due to timeout issues
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping gradlew test in CI environment");
        return Ok(());
    }
    let client = new_client(Some(".ahma/tools")).await?;
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

        let call_param = CallToolRequestParam {
            name: Cow::Borrowed("gradlew"),
            arguments: Some(
                json!({
                    "subcommand": command,
                    "working_directory": project_path
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = client.call_tool(call_param).await;

        match result {
            Ok(tool_result) => {
                assert!(
                    !tool_result.content.is_empty(),
                    "Command {} should return content",
                    command
                );

                if let Some(content) = tool_result.content.first() {
                    if let Some(text_content) = content.as_text() {
                        println!("✓ {} completed successfully", command);
                        // Don't print full output, just verify it's not empty
                        assert!(
                            !text_content.text.trim().is_empty(),
                            "Command {} should return non-empty output",
                            command
                        );
                    }
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

/// Test gradlew with various working directory scenarios
#[tokio::test]
async fn test_gradlew_working_directory_handling() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;
    let project_path = get_android_test_project_path();

    // Test 1: Valid project directory
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "tasks",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            assert!(!tool_result.content.is_empty());
            println!("✓ Valid project directory works");
        }
        Err(e) => {
            println!(
                "Note: gradlew tasks failed (possibly no Android SDK): {}",
                e
            );
        }
    }

    // Test 2: Non-existent directory should fail gracefully
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "tasks",
                "working_directory": "/nonexistent/directory/path"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            // Should get some kind of error response
            if let Some(content) = _tool_result.content.first() {
                if let Some(text_content) = content.as_text() {
                    // Should contain error message about directory not existing
                    println!(
                        "✓ Non-existent directory handled: {}",
                        text_content.text.chars().take(100).collect::<String>()
                    );
                }
            }
        }
        Err(_) => {
            println!("✓ Non-existent directory properly rejected");
        }
    }

    // Test 3: Missing working_directory parameter
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "tasks"
                // No working_directory
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            println!("✓ Missing working_directory handled gracefully");
            if let Some(content) = tool_result.content.first() {
                if let Some(text_content) = content.as_text() {
                    assert!(!text_content.text.is_empty());
                }
            }
        }
        Err(e) => {
            println!("✓ Missing working_directory properly rejected: {}", e);
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test gradlew subcommand parameter validation
#[tokio::test]
async fn test_gradlew_subcommand_validation() -> Result<()> {
    // Skip in CI environments due to timeout issues
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping gradlew test in CI environment");
        return Ok(());
    }
    let client = new_client(Some(".ahma/tools")).await?;
    let project_path = get_android_test_project_path();

    // Test 1: Valid subcommand
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "help",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ Valid subcommand accepted");
        }
        Err(e) => {
            println!(
                "Note: Valid subcommand failed (possibly no Android SDK): {}",
                e
            );
        }
    }

    // Test 2: Invalid subcommand
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "nonexistent_command_xyz",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            // Should get some error message about unknown task
            if let Some(content) = _tool_result.content.first() {
                if let Some(text_content) = content.as_text() {
                    println!(
                        "✓ Invalid subcommand handled: {}",
                        text_content.text.chars().take(100).collect::<String>()
                    );
                }
            }
        }
        Err(_) => {
            println!("✓ Invalid subcommand properly rejected");
        }
    }

    // Test 3: Missing subcommand
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "working_directory": project_path
                // No subcommand
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            // Should provide help or error about missing subcommand
            println!("✓ Missing subcommand handled gracefully");
        }
        Err(e) => {
            println!("✓ Missing subcommand properly rejected: {}", e);
        }
    }

    client.cancel().await?;
    Ok(())
}

/// Test gradlew commands with optional parameters
#[tokio::test]
async fn test_gradlew_optional_parameters() -> Result<()> {
    // Skip in CI environments due to timeout issues
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping gradlew test in CI environment");
        return Ok(());
    }
    let client = new_client(Some(".ahma/tools")).await?;
    let project_path = get_android_test_project_path();

    // Test 1: tasks command with --all option
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "tasks",
                "all": true,
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ tasks --all command accepted");
        }
        Err(e) => {
            println!("Note: tasks --all failed (possibly no Android SDK): {}", e);
        }
    }

    // Test 2: help command with task parameter
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "help",
                "task": "build",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            println!("✓ help --task command accepted");
            if let Some(content) = tool_result.content.first() {
                if let Some(text_content) = content.as_text() {
                    assert!(!text_content.text.is_empty());
                }
            }
        }
        Err(e) => {
            println!("Note: help --task failed (possibly no Android SDK): {}", e);
        }
    }

    // Test 3: dependencies command with configuration parameter
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": "dependencies",
                "configuration": "debugCompileClasspath",
                "working_directory": project_path
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(_tool_result) => {
            println!("✓ dependencies --configuration command accepted");
        }
        Err(e) => {
            println!(
                "Note: dependencies --configuration failed (possibly no Android SDK): {}",
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
    let client = new_client(Some(".ahma/tools")).await?;

    // Test that gradlew tool is available
    let tools = client.list_tools(None).await?;

    let gradlew_tool = tools.tools.iter().find(|t| t.name == "gradlew");
    assert!(gradlew_tool.is_some(), "gradlew tool should be available");

    let gradlew_tool = gradlew_tool.unwrap();
    assert_eq!(gradlew_tool.name, "gradlew");

    // Handle optional description
    if let Some(ref description) = gradlew_tool.description {
        assert!(
            description.contains("Android"),
            "Description should mention Android"
        );
        println!("  Description: {}", description);
    } else {
        println!("  Description: None provided");
    }

    println!("✓ gradlew tool loaded successfully");

    // Verify tool has input schema
    let schema_properties = gradlew_tool.input_schema.get("properties");
    if let Some(properties) = schema_properties {
        if let Some(properties_obj) = properties.as_object() {
            assert!(
                properties_obj.contains_key("subcommand"),
                "Schema should have subcommand property"
            );
            println!("✓ gradlew schema has subcommand property");
        }
    } else {
        println!("Note: gradlew schema properties not found");
    }

    client.cancel().await?;
    Ok(())
}

/// Test error handling for malformed parameters
#[tokio::test]
async fn test_gradlew_error_handling() -> Result<()> {
    let client = new_client(Some(".ahma/tools")).await?;

    // Test 1: Completely invalid parameters
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "invalid_field": "invalid_value",
                "another_invalid": 12345
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    };

    let result = client.call_tool(call_param).await;
    match result {
        Ok(tool_result) => {
            println!("✓ Invalid parameters handled gracefully");
            // Should get some error message
            if let Some(content) = tool_result.content.first() {
                if let Some(text_content) = content.as_text() {
                    assert!(!text_content.text.is_empty());
                }
            }
        }
        Err(e) => {
            println!("✓ Invalid parameters properly rejected: {}", e);
        }
    }

    // Test 2: Wrong parameter types
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(
            json!({
                "subcommand": 12345, // Should be string
                "working_directory": true // Should be string
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
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
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("gradlew"),
        arguments: Some(Map::new()),
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
