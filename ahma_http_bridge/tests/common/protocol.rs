//! JSON-RPC protocol types for MCP test clients.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic request ID counter for JSON-RPC
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Get next request ID
fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// JSON-RPC request structure
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    /// Create a new request with auto-generated ID
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(next_request_id()),
            method: method.to_string(),
            params,
        }
    }

    /// Create a notification (no ID, no response expected)
    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        }
    }

    /// Create an initialize request
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

    /// Create the initialized notification (required after initialize response)
    pub fn initialized() -> Self {
        Self::notification("notifications/initialized", json!({}))
    }

    /// Create a tools/call request
    pub fn call_tool(name: &str, arguments: Value) -> Self {
        Self::new(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments
            }),
        )
    }

    /// Create a tools/list request
    pub fn list_tools() -> Self {
        Self::new("tools/list", json!({}))
    }
}

/// JSON-RPC response structure
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
    /// Check if this response indicates success
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    /// Get the error message if present
    pub fn error_message(&self) -> Option<String> {
        self.error.as_ref().map(|e| e.message.clone())
    }

    /// Extract text content from a tools/call result
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

/// JSON-RPC error structure
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Result of a tool call
#[derive(Debug)]
pub struct ToolCallResult {
    pub tool_name: String,
    pub success: bool,
    pub duration_ms: u128,
    pub error: Option<String>,
    pub output: Option<String>,
}
