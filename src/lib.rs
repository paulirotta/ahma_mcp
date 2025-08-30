//! The main library for the Ahma MCP server.

// New core modules
pub mod adapter;
pub mod cli_parser;
pub mod config;
pub mod constants;
pub mod mcp_schema;
pub mod mcp_service;
pub mod utils;

// Modules copied from async_cargo_mcp
pub mod callback_system;
pub mod client;
pub mod mcp_callback;
pub mod operation_monitor;
pub mod shell_pool;
pub mod terminal_output;
pub mod tool_hints;

// Test utilities (conditionally compiled)
#[cfg(test)]
pub mod test;
