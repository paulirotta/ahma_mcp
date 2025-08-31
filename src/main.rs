//! # Ahma MCP Server Executable
//!
//! This is the main entry point for the `ahma_mcp` server application. It is responsible
//! for parsing command-line arguments, initializing the logging and configuration,
//! loading all the tool definitions, and starting the MCP server.
//!
//! ## Responsibilities
//!
//! - **Command-Line Argument Parsing**: Uses the `clap` crate to define and parse CLI
//!   arguments, such as the path to the tools directory, a flag to force synchronous
//!   operation, and the default command timeout.
//!
//! - **Logging Initialization**: Sets up the `tracing_subscriber` to provide structured
//!   logging. The log level can be controlled via the `--debug` flag.
//!
//! - **Tool Loading and Parsing**:
//!   1. Scans the specified `tools` directory for `.toml` configuration files.
//!   2. For each file, it loads the `Config` struct.
//!   3. It then uses the `CliParser` to execute the tool's `--help` command and parse
//!      the output into a `CliStructure`.
//!   4. Any failures during loading or parsing are logged as errors.
//!
//! - **Service Initialization**:
//!   1. Creates an `Adapter` instance, which will manage all tool execution.
//!   2. Initializes the `AhmaMcpService` with the adapter and the collection of loaded
//!      tool configurations and structures.
//!
//! - **Server Startup**: Calls `start_server()` on the `AhmaMcpService` instance, which
//!   binds to the appropriate address and begins listening for MCP client connections.
//!
//! ## Execution Flow
//!
//! 1. `main()` is invoked.
//! 2. `Cli::parse()` reads and validates command-line arguments.
//! 3. `tracing_subscriber` is configured.
//! 4. An `Adapter` is created.
//! 5. The `tools` directory is scanned, and each `.toml` file is processed to build a
//!    collection of `(tool_name, config, cli_structure)` tuples.
//! 6. `AhmaMcpService::new()` is called to create the service instance.
//! 7. `service.start_server()` is awaited, running the server indefinitely until it
//!    is shut down.

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
// TODO: Add comprehensive error handling examples// TODO: Performance optimization for large tool configurations
