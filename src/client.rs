//! MCP Client trait for abstraction.

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParam, CallToolResponse, ListToolsResponse};

/// A trait representing the capabilities of an MCP client.
/// This allows for mocking the client in tests.
#[async_trait]
pub trait McpClient: Send + Sync {
    async fn list_all_tools(&self) -> anyhow::Result<ListToolsResponse>;
    async fn call_tool(&self, params: CallToolRequestParam) -> anyhow::Result<CallToolResponse>;
}
