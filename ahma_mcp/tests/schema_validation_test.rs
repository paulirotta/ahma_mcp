//! Tests to validate that generated MCP tool schemas follow valid JSON Schema format.
//!
//! These tests ensure that the input schemas generated for MCP tools are valid JSON Schema
//! and do not contain invalid constructs like `"required": true` inside property definitions.
//!
//! JSON Schema spec: https://json-schema.org/understanding-json-schema/

use ahma_mcp::test_utils::test_client::new_client;
use ahma_mcp::utils::logging::init_test_logging;
use serde_json::{Map, Value};

/// Validates that a JSON Schema object follows proper JSON Schema conventions.
///
/// Key validations:
/// - `required` at schema level must be an array of strings
/// - `required` must NOT appear as a boolean inside property definitions
/// - Properties must have valid `type` fields
fn validate_json_schema_value(schema: &Value, tool_name: &str) -> Result<(), String> {
    let obj = schema
        .as_object()
        .ok_or_else(|| format!("Tool '{}': Schema is not an object", tool_name))?;
    validate_json_schema_map(obj, tool_name)
}

/// Validates a JSON Schema from a Map (for rmcp's Tool.input_schema which is Arc<Map<String, Value>>)
fn validate_json_schema_map(obj: &Map<String, Value>, tool_name: &str) -> Result<(), String> {
    // Validate top-level `required` is an array (if present)
    if let Some(required) = obj.get("required") {
        if !required.is_array() {
            return Err(format!(
                "Tool '{}': Top-level 'required' must be an array, got {:?}",
                tool_name, required
            ));
        }

        // Each element in required array must be a string
        if let Some(arr) = required.as_array() {
            for (i, item) in arr.iter().enumerate() {
                if !item.is_string() {
                    return Err(format!(
                        "Tool '{}': 'required[{}]' must be a string, got {:?}",
                        tool_name, i, item
                    ));
                }
            }
        }
    }

    // Validate properties don't have invalid 'required' boolean inside them
    if let Some(properties) = obj.get("properties")
        && let Some(props_obj) = properties.as_object()
    {
        for (prop_name, prop_schema) in props_obj {
            if let Some(prop_obj) = prop_schema.as_object() {
                // Check for invalid `required: true` or `required: false` inside property
                if let Some(req) = prop_obj.get("required")
                    && req.is_boolean()
                {
                    return Err(format!(
                        "Tool '{}': Property '{}' has invalid 'required: {}' inside property definition. \
                         In JSON Schema, 'required' must be an array at the schema level, not a boolean inside properties.",
                        tool_name, prop_name, req
                    ));
                }

                // Validate nested schemas (for arrays with items, etc.)
                if let Some(items) = prop_obj.get("items") {
                    validate_json_schema_value(
                        items,
                        &format!("{}.{}.items", tool_name, prop_name),
                    )?;
                }
            }
        }
    }

    // Validate oneOf/anyOf/allOf schemas
    for keyword in &["oneOf", "anyOf", "allOf"] {
        if let Some(schemas) = obj.get(*keyword)
            && let Some(arr) = schemas.as_array()
        {
            for (i, sub_schema) in arr.iter().enumerate() {
                validate_json_schema_value(
                    sub_schema,
                    &format!("{}.{}[{}]", tool_name, keyword, i),
                )?;
            }
        }
    }

    Ok(())
}

/// Test that all built-in tools have valid JSON Schema input schemas
#[tokio::test]
async fn test_builtin_tools_have_valid_json_schema() {
    init_test_logging();

    let client = new_client(Some(".ahma")).await.unwrap();
    let tools = client.list_tools(None).await.unwrap();

    let builtin_tools = ["await", "status", "sandboxed_shell"];

    for tool in tools.tools.iter() {
        if builtin_tools.contains(&tool.name.as_ref()) {
            let result = validate_json_schema_map(&tool.input_schema, &tool.name);
            assert!(
                result.is_ok(),
                "Built-in tool '{}' has invalid JSON Schema: {}",
                tool.name,
                result.unwrap_err()
            );
        }
    }

    client.cancel().await.unwrap();
}

/// Test that all user-defined tools have valid JSON Schema input schemas
#[tokio::test]
async fn test_all_tools_have_valid_json_schema() {
    init_test_logging();

    let client = new_client(Some(".ahma")).await.unwrap();
    let tools = client.list_tools(None).await.unwrap();

    let mut errors = Vec::new();

    for tool in tools.tools.iter() {
        if let Err(e) = validate_json_schema_map(&tool.input_schema, &tool.name) {
            errors.push(e);
        }
    }

    assert!(
        errors.is_empty(),
        "The following tools have invalid JSON Schema:\n{}",
        errors.join("\n")
    );

    client.cancel().await.unwrap();
}

/// Test specifically that sandboxed_shell schema is valid
/// This test catches the specific bug where `required: true` was placed inside property definitions
#[tokio::test]
async fn test_sandboxed_shell_schema_no_required_in_properties() {
    init_test_logging();

    let client = new_client(Some(".ahma")).await.unwrap();
    let tools = client.list_tools(None).await.unwrap();

    let sandboxed_shell = tools
        .tools
        .iter()
        .find(|t| t.name == "sandboxed_shell")
        .expect("sandboxed_shell tool should exist");

    // Check that the schema has required as an array at the top level
    let schema = &sandboxed_shell.input_schema;
    let required = schema
        .get("required")
        .expect("Schema should have 'required' field");
    assert!(
        required.is_array(),
        "sandboxed_shell: 'required' must be an array at schema level, got {:?}",
        required
    );

    // Check that properties don't have 'required' boolean inside them
    if let Some(properties) = schema.get("properties")
        && let Some(props_obj) = properties.as_object()
    {
        for (prop_name, prop_schema) in props_obj {
            if let Some(prop_obj) = prop_schema.as_object() {
                assert!(
                    !prop_obj.contains_key("required"),
                    "sandboxed_shell: Property '{}' should NOT have 'required' inside it. \
                     Use the top-level 'required' array instead.",
                    prop_name
                );
            }
        }
    }

    client.cancel().await.unwrap();
}
