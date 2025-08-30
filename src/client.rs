//! MCP Client trait for abstraction.

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParam, CallToolResult, ListToolsResult};

/// A trait representing the capabilities of an MCP client.
/// This allows for mocking the client in tests.
#[async_trait]
pub trait McpClient: Send + Sync {
    async fn list_all_tools(&self) -> anyhow::Result<ListToolsResult>;
    async fn call_tool(&self, params: CallToolRequestParam) -> anyhow::Result<CallToolResult>;
}
