//! The main entry point for the Ahma MCP server.

use ahma_mcp::utils::logging;
use clap::Parser;

/// Ahma MCP Server Command Line Interface
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the tool configuration TOML file
    #[arg(long, required = true)]
    config: String,

    /// Force all operations to run synchronously
    #[arg(long)]
    synchronous: bool,
}

fn main() {
    let cli = Cli::parse();

    // Initialize logging.
    if let Err(e) = logging::init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
    }

    // TODO: Use the parsed arguments
    tracing::info!("Config file: {}", cli.config);
    tracing::info!("Synchronous mode: {}", cli.synchronous);
}
