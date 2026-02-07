//! # HTTP Bridge Mode
//!
//! Runs the ahma_mcp server in HTTP bridge mode, which provides an HTTP interface
//! to the MCP server.

use crate::shell::cli::Cli;
use anyhow::{Context, Result};
use std::{env, path::PathBuf};

/// Run in HTTP bridge mode.
///
/// # Arguments
/// * `cli` - Command-line arguments.
///
/// # Errors
/// Returns an error if the bridge fails to start.
pub async fn run_http_bridge_mode(cli: Cli) -> Result<()> {
    use ahma_http_bridge::{BridgeConfig, start_bridge};

    // We need to re-derive fallback scope because we don't have global state anymore.
    // Ideally we pass `sandbox` to this function, but `sandbox` might be None if deferred.
    // So logic inside `run_http_bridge_mode` should just default to CWD if not provided,
    // OR we pass the calculated `sandbox_scopes` (if any) to it.

    let bind_addr = format!("{}:{}", cli.http_host, cli.http_port)
        .parse()
        .context("Invalid HTTP host/port")?;

    tracing::info!("Starting HTTP bridge on {}", bind_addr);
    tracing::info!("Session isolation: ENABLED (always-on)");

    // Build the command to run the stdio MCP server
    let server_command = env::current_exe()
        .context("Failed to get current executable path")?
        .to_string_lossy()
        .to_string();

    // Determine default scope
    // We can re-use the logic or just use CWD as fallback if not provided.
    // Since we don't have globals, we can't call get_sandbox_scopes.
    // Let's implement local logic:
    let default_scope = if !cli.sandbox_scope.is_empty() {
        // Use first CLI scope
        std::fs::canonicalize(&cli.sandbox_scope[0])
            .unwrap_or_else(|_| cli.sandbox_scope[0].clone())
    } else if let Ok(env_scope) = std::env::var("AHMA_SANDBOX_SCOPE") {
        PathBuf::from(env_scope)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    // Session isolation is always enabled in HTTP mode.
    let mut server_args = vec!["--mode".to_string(), "stdio".to_string()];

    // Only pass --tools-dir if explicitly provided
    if let Some(ref tools_dir) = cli.tools_dir {
        server_args.push("--tools-dir".to_string());
        server_args.push(tools_dir.to_string_lossy().to_string());
    }

    server_args.push("--timeout".to_string());
    server_args.push(cli.timeout.to_string());

    if cli.debug {
        server_args.push("--debug".to_string());
    }

    if cli.sync {
        server_args.push("--sync".to_string());
    }

    let enable_colored_output = true;
    tracing::info!(
        "HTTP bridge mode - colored terminal output enabled (v{})",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!(
        "HTTP default sandbox scope (used only if client provides no roots): {}",
        default_scope.display()
    );

    let config = BridgeConfig {
        bind_addr,
        server_command,
        server_args,
        enable_colored_output,
        default_sandbox_scope: default_scope,
        handshake_timeout_secs: cli.handshake_timeout_secs,
    };

    start_bridge(config).await?;

    Ok(())
}
