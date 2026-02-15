//! # Ahma HTTP Bridge
//!
//! A high-performance, secure HTTP-to-stdio bridge for MCP servers.
//!
//! This crate provides an HTTP server that proxies JSON-RPC requests to a
//! stdio-based MCP server subprocess, enabling HTTP clients to communicate
//! with MCP servers restricted to the standard input/output transport.
//!
//! ## Architecture
//!
//! The bridge operates in **Session Isolation Mode**, ensuring strict security boundaries:
//!
//! *   **Transport Proxy**: HTTP POST requests are converted to JSON-RPC over stdin/stdout.
//! *   **Session Management**: A dedicated subprocess is spawned for each client session (identified by `Mcp-Session-Id` header).
//! *   **Security**: Sandbox scope is dynamically bound to the client's workspace roots discovered during the handshake.
//!
//! ## Key Features
//!
//! *   **Streamable HTTP Transport**: Implements the MCP HTTP transport specification (2025-06-18), supporting POST for requests and SSE (Server-Sent Events) for server-to-client notifications.
//! *   **Strict by Default**: Clients must provide roots/list unless an explicit fallback scope is configured.
//! *   **Robust Error Handling**: Cleanly handles subprocess crashes and protocol violations.
//!
//! ## Example
//!
//! ```rust,no_run
//! use ahma_http_bridge::{BridgeConfig, start_bridge};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Configure the bridge
//!     let config = BridgeConfig {
//!         bind_addr: "127.0.0.1:3000".parse().unwrap(),
//!         server_command: "ahma_mcp".to_string(), // Path to your MCP server binary
//!         // Optional fallback for clients that do not support roots/list
//!         default_sandbox_scope: Some(PathBuf::from("/path/to/project")),
//!         ..BridgeConfig::default()
//!     };
//!     
//!     // Start the bridge server
//!     start_bridge(config).await?;
//!     Ok(())
//! }
//! ```

/// HTTP bridge server implementation.
pub mod bridge;
/// Error types for bridge operations.
pub mod error;
/// Session lifecycle management for HTTP clients.
pub mod session;

pub use bridge::{BridgeConfig, start_bridge};
pub use error::{BridgeError, Result};
pub use session::{
    DEFAULT_HANDSHAKE_TIMEOUT_SECS, McpRoot, Session, SessionManager, SessionManagerConfig,
    SessionTerminationReason,
};

/// Request handler for HTTP bridge.
pub mod request_handler;
