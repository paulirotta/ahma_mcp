//! # Server Modes Module
//!
//! Contains the different operational modes for the ahma_mcp server.

pub mod cli;
pub mod http_bridge;
pub mod list_tools;
pub mod server;

// Re-export mode functions for convenience
pub use cli::run_cli_mode;
pub use http_bridge::run_http_bridge_mode;
pub use list_tools::run_list_tools_mode;
pub use server::run_server_mode;
