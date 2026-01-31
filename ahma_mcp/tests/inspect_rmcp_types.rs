use rmcp::model::ClientCapabilities;
use serde_json::json;

#[test]
fn test_client_capabilities_deserialization() {
    // 1. Inspect what ClientCapabilities expects regarding "tasks"

    // Case A: Missing "tasks" field
    let json_missing = json!({
        "roots": { "listChanged": true }
    });
    let res_missing: Result<ClientCapabilities, _> = serde_json::from_value(json_missing);
    println!("Missing 'tasks': {:?}", res_missing);

    // Case B: "tasks" is boolean
    let json_bool = json!({
        "tasks": true,
        "roots": { "listChanged": true }
    });
    let res_bool: Result<ClientCapabilities, _> = serde_json::from_value(json_bool);
    println!("'tasks' = true: {:?}", res_bool);

    // Case C: "tasks" is object (VS Code style - The Failure Case)
    let json_obj = json!({
        "tasks": { "list": {} },
        "roots": { "listChanged": true }
    });
    let res_obj: Result<ClientCapabilities, _> = serde_json::from_value(json_obj);
    println!("'tasks' = object: {:?}", res_obj);
}
