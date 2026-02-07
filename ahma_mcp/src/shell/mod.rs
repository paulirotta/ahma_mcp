//! # Shell Module
//!
//! This module provides the main entry point and CLI logic for the `ahma_mcp` binary.
//! It includes command-line argument parsing, server modes (stdio, HTTP, CLI), and
//! tool listing functionality.
//!
//! ## Sub-modules
//!
//! - **`cli`**: Core CLI logic including argument parsing and mode dispatch
//! - **`list_tools`**: Functionality for listing MCP tools from servers

pub mod cli;
/// Utilities for listing MCP tools from servers.
pub mod list_tools;

// Re-export commonly used types
pub use list_tools::{
    McpConfig, OutputFormat, ParameterOutput, ServerConfig, ServerInfoOutput, ToolListResult,
    ToolOutput, expand_home, extract_parameters_from_json, list_tools_from_config, list_tools_http,
    list_tools_stdio_with_env, parse_mcp_config, print_json_output, print_text_output,
};

pub use cli::{
    find_matching_tool, find_tool_config, resolve_cli_subcommand, run, run_cli_sequence,
};
