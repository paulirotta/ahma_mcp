//! # Ahma HTTP Bridge
//!
//! A simple HTTP-to-stdio bridge for MCP servers.
//!
//! This crate provides an HTTP server that proxies JSON-RPC requests to a
//! stdio-based MCP server subprocess. This allows HTTP clients to communicate
//! with MCP servers that use the stdio transport.
//!
//! ## Session Isolation Mode (R8D)
//!
//! When session isolation is enabled (`--session-isolation`), each client gets
//! a separate subprocess with its own sandbox scope derived from the client's
//! workspace roots. This allows multiple IDE instances (VS Code, Cursor, etc.)
//! to share a single HTTP server while maintaining security isolation.
//!
//! ## Example
//!
//! ```rust,no_run
//! use ahma_http_bridge::{BridgeConfig, start_bridge};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = BridgeConfig {
//!         bind_addr: "127.0.0.1:3000".parse().unwrap(),
//!         server_command: "ahma_mcp".to_string(),
//!         server_args: vec!["--tools-dir".to_string(), "./tools".to_string()],
//!         enable_colored_output: false,
//!         session_isolation: false,
//!         default_sandbox_scope: PathBuf::from("."),
//!     };
//!     
//!     start_bridge(config).await?;
//!     Ok(())
//! }
//! ```

pub mod bridge;
pub mod error;
pub mod session;

pub use bridge::{BridgeConfig, start_bridge};
pub use error::{BridgeError, Result};
pub use session::{
    McpRoot, Session, SessionManager, SessionManagerConfig, SessionTerminationReason,
};
