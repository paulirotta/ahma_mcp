//! Test to reproduce and fix the VSCode GitHub Copilot Chat catastrophic failure
//! Error: "tool parameters array type must have items"
mod common;

use ahma_mcp::utils::logging::init_test_logging;
use common::test_client::new_client;
use futures::future::join_all;
use serde_json::Value;

/// This test reproduces the exact VSCode GitHub Copilot Chat failure
/// and ensures our fix prevents it from happening again.
#[tokio::test]
async fn test_array_parameters_must_have_items_property() -> anyhow::Result<()> {
    init_test_logging();
    // Create a test client with the real tool configurations (assume new_client is now async)
    let client = new_client(Some(".ahma/tools")).await?;
    let tools = client.list_all_tools().await?;

    eprintln!(
        "Testing {} tools for proper array schema generation",
        tools.len()
    );

    // Find cargo_audit tool which was causing the catastrophic failure
    let cargo_audit_tool = tools
        .iter()
        .find(|tool| tool.name == "cargo_audit")
        .expect("Should have cargo_audit tool - this was the tool causing VSCode GitHub Copilot Chat to fail");

    eprintln!("Found cargo_audit tool, checking its array parameters...");

    // The schema should be valid and not cause VSCode failures
    let schema = cargo_audit_tool.input_schema.as_ref();
    let properties = schema
        .get("properties")
        .expect("Tool schema must have properties")
        .as_object()
        .expect("Properties must be an object");

    // Collect array parameters for potential parallel validation
    let array_params: Vec<_> = properties
        .iter()
        .filter_map(|(param_name, param_schema)| {
            param_schema.as_object().and_then(|param_obj| {
                if param_obj.get("type") == Some(&Value::String("array".to_string())) {
                    Some((param_name.clone(), param_obj.clone()))
                } else {
                    None
                }
            })
        })
        .collect();

    // Validate array parameters (use join_all for concurrency if validation can be parallelized)
    let validation_futures: Vec<_> = array_params
        .into_iter()
        .map(|(param_name, param_obj)| async move {
            // CRITICAL FIX: Array parameters MUST have 'items' property
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

            eprintln!(
                "✅ Array parameter '{}' has valid items property",
                param_name
            );
            Ok(())
        })
        .collect();

    // Await all validations concurrently
    let results: Vec<Result<(), anyhow::Error>> = join_all(validation_futures).await;
    let validated_arrays = results.len();
    for result in results {
        result?; // Propagate any errors
    }
    assert!(
        validated_arrays >= 3,
        "Should have validated at least 3 array parameters in cargo_audit (ignore, target-arch, target-os)"
    );

    eprintln!(
        "✅ All {} array parameters have proper 'items' properties!",
        validated_arrays
    );

    client.cancel().await?;
    Ok(())
}
