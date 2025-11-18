//! # Ahma HTTP MCP Server
//!
//! A high-performance HTTP/3 and HTTP/2 server for the Model Context Protocol (MCP).
//!
//! This crate provides:
//! - HTTP/3 support using Quinn and h3
//! - HTTP/2 fallback using Axum
//! - Automatic self-signed certificate generation for localhost
//! - Server-Sent Events (SSE) for server-initiated messages
//! - Full MCP protocol support
//!
//! ## Features
//!
//! - **HTTP/3 First**: Uses QUIC for improved performance and multiplexing
//! - **Automatic Fallback**: Falls back to HTTP/2 if HTTP/3 is unavailable
//! - **Zero Configuration**: Automatically generates and caches TLS certificates
//! - **Localhost Only**: Designed for secure local development
//!
//! ## Example
//!
//! ```rust,no_run
//! use ahma_http_mcp_server::{ServerConfig, Protocol, start_server};
//! use ahma_core::AhmaMcpService;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = ServerConfig {
//!         protocol: Protocol::Http3WithFallback,
//!         ..Default::default()
//!     };
//!     
//!     let handler = AhmaMcpService::new(vec![])?;
//!     start_server(config, handler).await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod cert;
pub mod error;
pub mod handler;
pub mod http2_server;
pub mod http3_server;
pub mod server;

// Re-export main types
pub use error::{Result, ServerError};
pub use handler::McpServerState;
pub use server::{start_server, Protocol, ServerConfig};
