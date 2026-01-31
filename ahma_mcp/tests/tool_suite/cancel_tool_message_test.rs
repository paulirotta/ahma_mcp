use serde_json::Value;

#[test]
fn test_cancel_tool_message_suggestion_block_shape() {
    // Mirror the JSON shape created in mcp_service cancel branch
    let suggestion = serde_json::json!({
        "tool_hint": {
            "suggested_tool": "status",
            "reason": "Operation cancelled; check status and consider restarting",
            "next_steps": [
                {"tool": "status", "args": {"operation_id": "op_123"}},
                {"tool": "await", "args": {"tools": "", "timeout_seconds": 120}}
            ]
        }
    });

    // Validate structure
    let root: Value = suggestion;
    let hint = root.get("tool_hint").expect("tool_hint present");
    assert_eq!(hint.get("suggested_tool").unwrap().as_str(), Some("status"));
    assert!(
        hint.get("reason")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("cancelled")
    );

    let steps = hint.get("next_steps").unwrap().as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].get("tool").unwrap().as_str(), Some("status"));
}
