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
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, instrument};
use tracing_subscriber::EnvFilter;

use ahma_mcp::{
    adapter::Adapter, cli_parser::CliParser, config::Config, mcp_service::AhmaMcpService,
};

/// Ahma MCP Server: Universal CLI Tool Adapter for AI Agents
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "ahma_mcp can run in two modes:
1. Server Mode: Runs as a persistent MCP server.
   Example: ahma_mcp --server

2. CLI Mode: Executes a single command and exits.
   Example: ahma_mcp ls_run --working-directory . -- -l -a"
)]
struct Cli {
    /// Run in persistent MCP server mode.
    #[arg(long)]
    server: bool,

    /// Path to the tools directory containing TOML configuration files.
    #[arg(long, global = true, default_value = "tools")]
    tools_dir: PathBuf,

    /// Force all operations to run synchronously.
    #[arg(long, global = true)]
    synchronous: bool,

    /// Timeout for commands in seconds.
    #[arg(long, global = true, default_value = "300")]
    timeout: u64,

    /// Enable debug logging.
    #[arg(short, long, global = true)]
    debug: bool,

    /// The name of the tool to execute (e.g., 'ls_run', 'cargo_build').
    #[arg()]
    tool_name: Option<String>,

    /// Arguments for the tool.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    tool_args: Vec<String>,
}

#[tokio::main]
#[instrument]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(format!("ahma_mcp={}", log_level)))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    if cli.server || cli.tool_name.is_none() {
        info!("Running in Server mode");
        run_server_mode(cli).await
    } else {
        info!("Running in CLI mode");
        run_cli_mode(cli).await
    }
}

async fn run_server_mode(cli: Cli) -> Result<()> {
    info!("Starting ahma_mcp v0.1.0");
    info!("Tools directory: {:?}", cli.tools_dir);
    info!("Synchronous mode: {}", cli.synchronous);
    info!("Command timeout: {}s", cli.timeout);

    // Initialize the adapter
    let adapter = Arc::new(Adapter::with_timeout(cli.synchronous, cli.timeout)?);

    // Load tool configurations
    let tools = load_tools(&cli.tools_dir).await?;

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

async fn run_cli_mode(cli: Cli) -> Result<()> {
    let tool_name = cli.tool_name.unwrap(); // Safe to unwrap due to check in main

    // Build a fresh adapter and register tools
    let mut adapter = Adapter::with_timeout(cli.synchronous, cli.timeout)?;
    let tools = load_tools(&cli.tools_dir).await?;
    for (name, config, cli_structure) in tools {
        adapter.register_tool(&name, config, cli_structure);
    }

    // Construct arguments from the remaining parts
    let mut arguments = serde_json::Map::new();
    let mut raw_args = Vec::new();
    let mut iter = cli.tool_args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        if arg == "--" {
            // Treat the rest as positional args verbatim
            for rest in iter {
                raw_args.push(Value::String(rest));
            }
            break;
        }
        if let Some(key) = arg.strip_prefix("--") {
            if let Some(next_arg) = iter.peek() {
                if !next_arg.starts_with('-') {
                    // Value argument (e.g., --key value)
                    arguments.insert(key.to_string(), Value::String(iter.next().unwrap()));
                } else {
                    // Flag (e.g., --release)
                    arguments.insert(key.to_string(), Value::Bool(true));
                }
            } else {
                // Flag at the end
                arguments.insert(key.to_string(), Value::Bool(true));
            }
        } else {
            raw_args.push(Value::String(arg));
        }
    }

    if !raw_args.is_empty() {
        arguments.insert("args".to_string(), Value::Array(raw_args));
    }

    // Default working_directory to current dir if not provided
    if !arguments.contains_key("working_directory") {
        let current_dir = std::env::current_dir()?.to_string_lossy().to_string();
        arguments.insert("working_directory".to_string(), Value::String(current_dir));
    }

    // Directly execute via Adapter to avoid MCP conversion layers
    let mut args_vec = if let Some(Value::Array(vals)) = arguments.remove("args") {
        vals.into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let working_directory = arguments
        .remove("working_directory")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    // Convert remaining key/value pairs in `arguments` into CLI flags
    let mut flags_vec: Vec<String> = Vec::new();
    for (k, v) in arguments.into_iter() {
        match v {
            Value::Bool(true) => {
                flags_vec.push(format!("--{}", k));
            }
            Value::String(s) => {
                flags_vec.push(format!("--{}", k));
                flags_vec.push(s);
            }
            Value::Array(arr) => {
                // Support multi-value flags: --key v1 --key v2
                for item in arr {
                    if let Some(s) = item.as_str() {
                        flags_vec.push(format!("--{}", k));
                        flags_vec.push(s.to_string());
                    }
                }
            }
            _ => {
                // Ignore unsupported types in CLI mode
            }
        }
    }
    // Prepend flags so options appear before positional args (e.g., git push)
    if !flags_vec.is_empty() {
        let mut combined = Vec::with_capacity(flags_vec.len() + args_vec.len());
        combined.extend(flags_vec);
        combined.extend(args_vec);
        args_vec = combined;
    }

    let base_tool = tool_name.split('_').next().unwrap_or("");
    // Insert subcommands if present (e.g., cargo_nextest_run -> ["nextest", "run"], cargo_build -> ["build"]).
    // For single-command tools like ls_run, suppress a sole "run".
    let subcmd_parts: Vec<String> = tool_name
        .split('_')
        .skip(1)
        .map(|s| s.to_string())
        .collect();
    if !(subcmd_parts.is_empty() || (subcmd_parts.len() == 1 && subcmd_parts[0] == "run")) {
        let mut with_subcmd = Vec::with_capacity(subcmd_parts.len() + args_vec.len());
        with_subcmd.extend(subcmd_parts);
        with_subcmd.extend(args_vec);
        let exec_result = adapter
            .execute_tool_in_dir(base_tool, with_subcmd, working_directory)
            .await;
        return match exec_result {
            Ok(output) => {
                println!("{}", output);
                Ok(())
            }
            Err(e) => {
                eprintln!("Error executing tool: {}", e);
                Err(anyhow::anyhow!("Tool execution failed"))
            }
        };
    }

    let exec_result = adapter
        .execute_tool_in_dir(base_tool, args_vec, working_directory)
        .await;

    match exec_result {
        Ok(output) => {
            println!("{}", output);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error executing tool: {}", e);
            Err(anyhow::anyhow!("Tool execution failed"))
        }
    }
}

async fn load_tools(
    tools_dir: &PathBuf,
) -> Result<Vec<(String, Config, ahma_mcp::cli_parser::CliStructure)>> {
    let mut tools = Vec::new();
    if !tools_dir.exists() {
        error!("Tools directory does not exist: {:?}", tools_dir);
        return Ok(tools); // Return empty, don't exit
    }

    let mut entries = tokio::fs::read_dir(tools_dir).await?;
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
    Ok(tools)
}
// TODO: Add comprehensive error handling examples// TODO: Performance optimization for large tool configurations
