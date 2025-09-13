//! Test to reproduce and fix the VSCode GitHub Copilot Chat catastrophic failure
//! Error: "tool parameters array type must have items"
mod common;

use ahma_mcp::utils::logging::init_test_logging;
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

    // Find cargo_audit tool which was causing the catastrophic failure
    let cargo_audit_tool = tools
        .iter()
        .find(|tool| tool.name == "cargo_audit")
        .expect("Should have cargo_audit tool - this was the tool causing VSCode GitHub Copilot Chat to fail");

    println!("Found cargo_audit tool, checking its array parameters...");

    // The schema should be valid and not cause VSCode failures
    let schema = cargo_audit_tool.input_schema.as_ref();
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

    assert!(
        validated_arrays >= 3,
        "Should have validated at least 3 array parameters in cargo_audit (ignore, target-arch, target-os)"
    );

    println!(
        "✅ All {} array parameters have proper 'items' properties!",
        validated_arrays
    );

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
