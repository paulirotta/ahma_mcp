//! # Ahma MCP Lib
//!
//! This crate provides the core functionality for the `ahma_mcp` server and CLI.
//! It is structured into several modules, each responsible for a distinct part of the
//! application's logic.
//!
//! ## Modules
//!
//! - **`adapter`**: Contains the `Adapter` struct, which is the primary engine for
//!   executing external command-line tools. It manages a `ShellPool` to run commands
//!   concurrently and safely, handling command construction, execution, and output
//!   capture.
//!
//! - **`config`**: Defines the data structures for tool configuration, primarily the
//!   `Config`, `Subcommand`, and `CliOption` structs. These are deserialized from TOML
//!   files and provide the blueprint for how `ahma_mcp` understands and interacts with
//!   a CLI tool.
//!
//! - **`constants`**: A central place for defining application-wide constants, such as
//!   default timeout values or common string literals.
//!
//! - **`mcp_service`**: Implements the `rmcp::ServerHandler` trait in the `AhmaMcpService`
//!   struct. This is the core of the MCP server, responsible for handling requests like
//!   `get_info`, `list_tools`, and `call_tool`. It uses the loaded configurations to
//!   dynamically generate tool definitions and execute commands via the `Adapter`.
//!
//! - **`operation_monitor`**: Provides a system for tracking the progress of long-running,
//!   asynchronous operations. It allows the server to send notifications back to the
//!   client about the status of background tasks.
//!
//! - **`shell_pool`**: A resource management utility that maintains a pool of reusable
//!   shell processes (`zsh`). This avoids the overhead of spawning a new shell for every
//!   command, improving performance, especially under heavy load.
//!
//! - **`terminal_output`**: Contains helpers for processing and sanitizing raw terminal
//!   output, such as stripping ANSI escape codes to provide clean text to the client.
//!
//! - **`tool_hints`**: Logic for generating helpful, context-aware hints that are appended
//!   to tool descriptions, guiding the user on best practices (e.g., using async for
//!   long-running commands).
//!
//! - **`utils`**: A collection of miscellaneous utility functions used across the
//!   application.

// Public modules
pub mod adapter;
pub mod callback_system;
pub mod client;
pub mod config;
pub mod constants;
pub mod logging;
pub mod mcp_callback;
pub mod mcp_service;
pub mod operation_monitor;
pub mod shell_pool;
pub mod terminal_output;
pub mod tool_hints;
pub mod utils;

// Test utilities (conditionally compiled)
#[cfg(test)]
pub mod test;

// Re-export main types for easier use
pub use adapter::Adapter;
pub use config::Config;
pub use mcp_service::AhmaMcpService;
