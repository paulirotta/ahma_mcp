//! # Ahma MCP Server Library
//!
//! This crate serves as the core library for the `ahma_mcp` server, a dynamic and
//! extensible tool server that conforms to the Machine-Checked Protocol (MCP). It is
//! designed to expose command-line tools to an AI agent in a structured and efficient
//! manner.
//!
//! ## Core Architecture
//!
//! The library is built around a few key modules:
//!
//! - **`mcp_service`**: The main entry point of the server. The `AhmaMcpService` struct
//!   implements the `rmcp` server trait and orchestrates all incoming requests.
//!
//! - **`adapter`**: The heart of the dynamic tool system. The `Adapter` is responsible for
//!   discovering, parsing, and executing CLI tools based on `.toml` configuration files.
//!
//! - **`config`**: Defines the structures for tool configuration, allowing for declarative
//!   definition of tools, their behavior, and AI-specific hints.
//!
//! - **`cli_parser`**: A utility that parses the `--help` output of commands to dynamically
//!   understand their subcommands and options.
//!
//! - **`mcp_schema`**: Generates MCP-compliant JSON schemas from the parsed CLI structures,
//!   enabling the AI client to understand how to use the tools.
//!
//! - **`shell_pool`**: A high-performance, asynchronous shell pooling system that minimizes
//!   latency for command execution by reusing pre-warmed shell processes.
//!
//! - **`callback_system`** and **`operation_monitor`**: These modules provide the framework
//!   for managing and reporting the progress of long-running asynchronous operations.
//!
//! ## Key Features
//!
//! - **Dynamic Tooling**: New CLI tools can be integrated by simply adding a TOML file to
//!   the `tools/` directory, without any changes to the Rust code.
//! - **Asynchronous by Default**: Leverages `tokio` and a custom shell pool to execute
//!   commands asynchronously, enabling high concurrency and responsiveness.
//! - **AI-Centric Design**: Includes features like `ToolHints` and dynamic guidance to help
//!   AI agents use the exposed tools more effectively.
//! - **MCP Compliance**: Implements the standard MCP interfaces for tool discovery and
-   execution, ensuring compatibility with MCP clients.
//!
//! This library brings together these components to create a robust and flexible foundation
//! for the `ahma_mcp` server.

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

// Re-export main types for easier use
pub use adapter::Adapter;
pub use cli_parser::{CliParser, CliStructure};
pub use config::Config;
pub use mcp_schema::McpSchemaGenerator;
pub use mcp_service::AhmaMcpService;
