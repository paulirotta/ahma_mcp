//! # List Tools Mode
//!
//! Runs the ahma_mcp server in list-tools mode, which connects to an MCP server
//! and lists all available tools.

use crate::shell::{cli::Cli, list_tools};
use anyhow::{Result, anyhow};
use std::collections::HashMap;

/// Run in list-tools mode: connect to an MCP server and list all available tools.
///
/// # Arguments
/// * `cli` - Command-line arguments.
///
/// # Errors
/// Returns an error if the connection or listing fails.
pub async fn run_list_tools_mode(cli: &Cli) -> Result<()> {
    // Determine connection mode
    let result = if let Some(http_url) = &cli.http {
        list_tools::list_tools_http(http_url).await?
    } else if cli.tool_name.is_some() || !cli.tool_args.is_empty() {
        // Build command args from tool_name (first positional) and tool_args (after --)
        let mut command_args: Vec<String> = Vec::new();
        if let Some(ref cmd) = cli.tool_name {
            command_args.push(cmd.clone());
        }
        command_args.extend(cli.tool_args.clone());

        if command_args.is_empty() {
            return Err(anyhow!(
                "No command specified for --list-tools. Provide command after --"
            ));
        }

        list_tools::list_tools_stdio_with_env(&command_args, HashMap::new()).await?
    } else if cli.mcp_config.exists() {
        list_tools::list_tools_from_config(&cli.mcp_config, cli.server.as_deref()).await?
    } else {
        return Err(anyhow!(
            "No connection method specified for --list-tools. Use --http, --mcp-config with --server, or provide command after --"
        ));
    };

    // Output result
    match cli.format {
        list_tools::OutputFormat::Text => list_tools::print_text_output(&result),
        list_tools::OutputFormat::Json => list_tools::print_json_output(&result)?,
    }

    Ok(())
}
