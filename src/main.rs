//! The main entry point for the Ahma MCP server.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use ahma_mcp::{
    adapter::Adapter, cli_parser::CliParser, config::Config, mcp_service::AhmaMcpService,
};

/// Ahma MCP Server Command Line Interface
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the tools directory containing TOML configuration files  
    #[arg(long, default_value = "tools")]
    tools_dir: PathBuf,

    /// Force all operations to run synchronously
    #[arg(long)]
    synchronous: bool,

    /// Timeout for commands in seconds
    #[arg(long, default_value = "300")]
    timeout: u64,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(format!("ahma_mcp={}", log_level)))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    info!("Starting ahma_mcp v0.1.0");
    info!("Tools directory: {:?}", cli.tools_dir);
    info!("Synchronous mode: {}", cli.synchronous);
    info!("Command timeout: {}s", cli.timeout);

    // Initialize the adapter
    let adapter = Arc::new(Adapter::with_timeout(cli.synchronous, cli.timeout)?);

    // Load tool configurations
    let mut tools = Vec::new();

    if !cli.tools_dir.exists() {
        error!("Tools directory does not exist: {:?}", cli.tools_dir);
        std::process::exit(1);
    }

    // Read all .toml files in the tools directory
    let mut entries = tokio::fs::read_dir(&cli.tools_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let tool_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            info!("Loading tool configuration: {}", tool_name);

            match Config::load_from_file(&path) {
                Ok(config) => {
                    let cli_parser = CliParser::new()?;

                    match cli_parser.parse_tool_with_config(&config).await {
                        Ok(cli_structure) => {
                            info!("Successfully parsed CLI structure for {}", tool_name);
                            tools.push((tool_name, config, cli_structure));
                        }
                        Err(e) => {
                            error!("Failed to parse CLI structure for {}: {}", tool_name, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to load config for {}: {}", tool_name, e);
                }
            }
        }
    }

    if tools.is_empty() {
        error!("No valid tool configurations found in {:?}", cli.tools_dir);
        std::process::exit(1);
    }

    info!("Loaded {} tool configurations", tools.len());

    // Create and start the MCP service
    let service = AhmaMcpService::new(adapter, tools).await?;
    service.start_server().await?;

    Ok(())
}
