//! Test to reproduce and fix the VSCode GitHub Copilot Chat catastrophic failure
//! Error: "tool parameters array type must have items"
mod common;

use ahma_core::utils::logging::init_test_logging;
use common::test_client::new_client;
use serde_json::Value;

/// This test reproduces the exact VSCode GitHub Copilot Chat failure
/// and ensures our fix prevents it from happening again.
#[tokio::test]
async fn test_array_parameters_have_items_property_fixed() -> anyhow::Result<()> {
    init_test_logging();
    // Create a test client with the real tool configurations
    let client = new_client(Some(".ahma/tools")).await?;
    let tools = client.list_all_tools().await?;

    println!(
        "Testing {} tools for proper array schema generation",
        tools.len()
    );

    // Inspect cargo tool which now includes audit subcommands
    let cargo_tool = tools
        .iter()
        .find(|tool| tool.name == "cargo")
        .expect("cargo tool should exist after consolidation");

    println!("Found cargo tool, checking audit-related array parameters...");

    // The schema should be valid and not cause VSCode failures
    let schema = cargo_tool.input_schema.as_ref();
    let properties = schema
        .get("properties")
        .expect("Tool schema must have properties")
        .as_object()
        .expect("Properties must be an object");

    // These specific array parameters were causing the VSCode failure
    let mut validated_arrays = 0;

    for (param_name, param_schema) in properties {
        if let Some(param_obj) = param_schema.as_object() {
            if param_obj.get("type") == Some(&Value::String("array".to_string())) {
                validated_arrays += 1;
                println!("Validating array parameter: {}", param_name);

                // CRITICAL FIX: Array parameters MUST have 'items' property
                // This is what was missing and causing the catastrophic failure
                assert!(
                    param_obj.contains_key("items"),
                    "CRITICAL: Array parameter '{}' MUST have 'items' property! \
                     This is what caused VSCode GitHub Copilot Chat to fail with: \
                     'tool parameters array type must have items'",
                    param_name
                );

                let items = param_obj
                    .get("items")
                    .expect("Items must be present")
                    .as_object()
                    .expect("Items must be an object");

                assert!(
                    items.contains_key("type"),
                    "Array items must have a type for parameter '{}'",
                    param_name
                );

                // For command-line tools, array items should typically be strings
                assert_eq!(
                    items.get("type").unwrap(),
                    &Value::String("string".to_string()),
                    "Array items should be strings for CLI parameter '{}'",
                    param_name
                );

                println!(
                    "✅ Array parameter '{}' has valid items property",
                    param_name
                );
            }
        }
    }

    // Check if cargo-audit is installed
    let audit_installed = std::process::Command::new("cargo")
        .args(["audit", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if audit_installed {
        assert!(
            validated_arrays >= 5,
            "Should have validated at least 5 array parameters with cargo audit installed (ignore, target-arch, target-os, args, exclude)"
        );
        println!(
            "cargo-audit is installed, validated {} array parameters.",
            validated_arrays
        );
    } else {
        assert!(
            validated_arrays >= 2,
            "Should have validated at least 2 array parameters without cargo audit installed (args, exclude)"
        );
        println!(
            "cargo-audit not found, skipping audit-related parameter checks. Validated {} array parameters.",
            validated_arrays
        );
    }

    client.cancel().await?;
    Ok(())
}

/// Test all tools to ensure none have the array schema issue
#[tokio::test]
async fn test_all_tools_array_schemas_are_valid_fixed() -> anyhow::Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;
    let tools = client.list_all_tools().await?;

    let mut total_tools = 0;
    let mut tools_with_arrays = 0;
    let mut total_array_params = 0;

    for tool in &tools {
        total_tools += 1;
        let schema = tool.input_schema.as_ref();

        if let Some(properties) = schema.get("properties") {
            let props = properties
                .as_object()
                .expect("Properties must be an object");

            let mut tool_has_arrays = false;
            for (param_name, param_schema) in props {
                if let Some(param_obj) = param_schema.as_object() {
                    if param_obj.get("type") == Some(&Value::String("array".to_string())) {
                        tool_has_arrays = true;
                        total_array_params += 1;

                        // THE CRITICAL TEST: Array parameters must have items property
                        assert!(
                            param_obj.contains_key("items"),
                            "Array parameter '{}' in tool '{}' MUST have 'items' property \
                             to prevent VSCode GitHub Copilot Chat catastrophic failure",
                            param_name,
                            tool.name
                        );

                        let items = param_obj.get("items").unwrap();
                        if let Some(items_obj) = items.as_object() {
                            assert!(
                                items_obj.contains_key("type"),
                                "Array items for '{}' in tool '{}' must have type field",
                                param_name,
                                tool.name
                            );
                        }
                    }
                }
            }

            if tool_has_arrays {
                tools_with_arrays += 1;
            }
        }
    }

    println!("✅ Schema validation results:");
    println!("   - Total tools tested: {}", total_tools);
    println!("   - Tools with array parameters: {}", tools_with_arrays);
    println!("   - Total array parameters: {}", total_array_params);
    println!("   - All array parameters have valid 'items' properties!");

    client.cancel().await?;
    Ok(())
}
