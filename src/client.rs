//! # MCP Client Abstraction
//!
//! This module defines a trait for an MCP (Machine-Checked Protocol) client.
//! By abstracting the client's capabilities, it allows for easier testing and
//! potential integration with different MCP client implementations.
//!
//! ## Core Components
//!
//! - **`McpClient`**: An `async_trait` that specifies the essential functions an MCP
//!   client must provide:
//!   - `list_all_tools()`: To discover the tools available on the server.
//!   - `call_tool()`: To execute a specific tool with given parameters.
//!
//! ## Purpose
//!
//! The primary purpose of this abstraction is to decouple the server's logic from any
//! concrete MCP client implementation. This is particularly useful for unit and
//! integration testing, where a mock `McpClient` can be substituted to simulate
//! client behavior without needing a full network connection.

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParam, CallToolResult, ListToolsResult};

/// A trait representing the capabilities of an MCP client.
/// This allows for mocking the client in tests.
#[async_trait]
pub trait McpClient: Send + Sync {
    async fn list_all_tools(&self) -> anyhow::Result<ListToolsResult>;
    async fn call_tool(&self, params: CallToolRequestParam) -> anyhow::Result<CallToolResult>;
}
