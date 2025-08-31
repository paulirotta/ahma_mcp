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

use ahma_mcp::{
    adapter::{Adapter, ExecutionMode},
    config::Config,
    mcp_service::AhmaMcpService,
};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, instrument};
use tracing_subscriber::EnvFilter;

/// Ahma MCP Server: A generic, config-driven adapter for CLI tools.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "ahma_mcp runs in two modes:
1. Server Mode: Runs as a persistent MCP server over stdio.
   Example: ahma_mcp --server

2. CLI Mode: Executes a single command and prints the result to stdout.
   Example: ahma_mcp cargo_build --working-directory . -- --release"
)]
struct Cli {
    /// Run in persistent MCP server mode.
    #[arg(long)]
    server: bool,

    /// Path to the directory containing tool TOML configuration files.
    #[arg(long, global = true, default_value = "tools")]
    tools_dir: PathBuf,

    /// Force all operations to run synchronously.
    #[arg(long, global = true)]
    synchronous: bool,

    /// Default timeout for commands in seconds.
    #[arg(long, global = true, default_value = "300")]
    timeout: u64,

    /// Enable debug logging.
    #[arg(short, long, global = true)]
    debug: bool,

    /// The name of the tool to execute (e.g., 'cargo_build').
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
    info!("Starting ahma_mcp v1.0.0");
    info!("Tools directory: {:?}", cli.tools_dir);
    info!("Synchronous mode: {}", cli.synchronous);
    info!("Command timeout: {}s", cli.timeout);

    // Initialize the adapter
    let adapter = Arc::new(Adapter::with_timeout(cli.synchronous, cli.timeout)?);

    // Load tool configurations
    let configs = load_tool_configs(&cli.tools_dir).await?;
    if configs.is_empty() {
        error!("No valid tool configurations found in {:?}", cli.tools_dir);
        // It's not a fatal error to have no tools, just log it.
    } else {
        info!("Loaded {} tool configurations", configs.len());
    }

    // Create and start the MCP service
    let service = AhmaMcpService::new(adapter, configs).await?;
    service.start_server().await?;

    Ok(())
}

async fn run_cli_mode(cli: Cli) -> Result<()> {
    let tool_name = cli.tool_name.unwrap(); // Safe due to check in main()

    // Initialize adapter
    let adapter = Adapter::with_timeout(cli.synchronous, cli.timeout)?;

    // Load the specific tool's config to check for sync override
    let parts: Vec<&str> = tool_name.split('_').collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid tool name format. Expected 'tool_subcommand'.");
    }
    let base_tool = parts[0];
    let subcommand_name = parts[1..].join("_");

    let config = Config::load_tool_config(base_tool)?;
    let subcommand_config = config
        .subcommand
        .iter()
        .find(|sc| sc.name == subcommand_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Subcommand '{}' not found for tool '{}'",
                subcommand_name,
                base_tool
            )
        })?;

    // Construct arguments - start with subcommand name, then add config args, then runtime args
    let mut raw_args = Vec::new();

    // Only add subcommand name if it's different from the base tool name
    // This handles cases like ls_ls where command="ls" and subcommand="ls"
    if subcommand_name != base_tool {
        raw_args.push(subcommand_name.clone());
    }

    // Add predefined args from subcommand config
    raw_args.extend(subcommand_config.args.clone());

    let mut working_directory: Option<String> = None;
    let mut tool_args_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Check for environment variable for programmatic execution
    if let Ok(env_args) = std::env::var("AHMA_MCP_ARGS") {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&env_args)
            && let Some(map) = json_val.as_object()
        {
            tool_args_map = map.clone();
        }
    } else {
        // Manual parsing for CLI invocation
        let mut iter = cli.tool_args.into_iter();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                raw_args.extend(iter.map(|s| s.to_string()));
                break;
            }
            if arg.starts_with("--") {
                let key = arg.trim_start_matches("--").to_string();
                if let Some(val) = iter.next() {
                    if key == "working-directory" {
                        working_directory = Some(val);
                    } else {
                        tool_args_map.insert(key, serde_json::Value::String(val));
                    }
                } else {
                    tool_args_map.insert(key, serde_json::Value::Bool(true));
                }
            } else {
                raw_args.push(arg);
            }
        }
    }

    if working_directory.is_none()
        && let Some(wd) = tool_args_map
            .get("working_directory")
            .and_then(|v| v.as_str())
    {
        working_directory = Some(wd.to_string());
    }

    if let Some(args_from_map) = tool_args_map.get("args").and_then(|v| v.as_array()) {
        raw_args.extend(
            args_from_map
                .iter()
                .filter_map(|v| v.as_str().map(String::from)),
        );
    }

    let final_working_dir = working_directory.or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });

    // CLI mode is always synchronous in its behavior, but we respect the config
    // to decide *how* it runs. However, for the user, it's a blocking call.
    let exec_mode = if cli.synchronous || subcommand_config.synchronous.unwrap_or(false) {
        ExecutionMode::Synchronous
    } else {
        // In CLI mode, even "async" commands should be awaited.
        // We can treat it as synchronous from the user's perspective.
        ExecutionMode::Synchronous
    };

    // Execute the tool
    let result = adapter
        .execute_tool_in_dir(
            base_tool,
            raw_args,
            final_working_dir,
            exec_mode,
            None, // No hints in CLI mode
        )
        .await;

    match result {
        Ok(output) => {
            // Extract and print the text content from the result
            for content in output.content {
                if let rmcp::model::RawContent::Text(text) = content.raw {
                    println!("{}", text.text);
                }
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Error executing tool: {}", e);
            Err(anyhow::anyhow!("Tool execution failed"))
        }
    }
}

/// Loads all valid `.toml` tool configurations from the specified directory.
async fn load_tool_configs(tools_dir: &PathBuf) -> Result<Vec<(String, Config)>> {
    let mut configs = Vec::new();
    if !tools_dir.exists() {
        error!("Tools directory does not exist: {:?}", tools_dir);
        return Ok(configs);
    }

    info!("Scanning tools directory: {:?}", tools_dir);
    let mut entries = tokio::fs::read_dir(tools_dir).await?;
    let mut toml_files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            toml_files.push(path);
        }
    }

    info!("Found {} .toml files in tools directory", toml_files.len());

    for path in toml_files {
        let tool_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        info!("Loading configuration for tool: {}", tool_name);
        debug!("Reading config file: {:?}", path);

        match Config::load_from_file(&path) {
            Ok(config) => {
                if config.enabled.unwrap_or(true) {
                    info!(
                        "Successfully loaded and enabled tool: {} (command: {}, {} subcommands)",
                        tool_name,
                        config.command,
                        config.subcommand.len()
                    );

                    // Log subcommand details for debugging
                    for subcommand in &config.subcommand {
                        debug!(
                            "  - Subcommand '{}': {} (options: {})",
                            subcommand.name,
                            subcommand.description,
                            subcommand.options.len()
                        );
                    }

                    configs.push((tool_name, config));
                } else {
                    info!("Skipping disabled tool: {}", tool_name);
                }
            }
            Err(e) => {
                error!("Failed to load config for {}: {}", tool_name, e);
                debug!("Config file path: {:?}", path);
            }
        }
    }

    info!("Successfully loaded {} tool configurations", configs.len());
    Ok(configs)
}
