use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(next_request_id()),
            method: method.to_string(),
            params,
        }
    }

    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        }
    }

    pub fn initialize(client_name: &str) -> Self {
        Self::new(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"roots": {}},
                "clientInfo": {"name": client_name, "version": "1.0"}
            }),
        )
    }

    pub fn initialized() -> Self {
        Self::notification("notifications/initialized", json!({}))
    }

    pub fn call_tool(name: &str, arguments: Value) -> Self {
        Self::new(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments
            }),
        )
    }

    pub fn list_tools() -> Self {
        Self::new("tools/list", json!({}))
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    pub fn error_message(&self) -> Option<String> {
        self.error.as_ref().map(|e| e.message.clone())
    }

    pub fn extract_tool_output(&self) -> Option<String> {
        self.result.as_ref().and_then(|r| {
            r.get("content").and_then(|c| c.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}
