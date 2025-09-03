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
//!   1. Scans the specified `tools` directory for `.json` configuration files.
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
//! 5. The `tools` directory is scanned, and each `.json` file is processed to build a
//!    collection of `(tool_name, config, cli_structure)` tuples.
//! 6. `AhmaMcpService::new()` is called to create the service instance.
//! 7. `service.start_server()` is awaited, running the server indefinitely until it
//!    is shut down.

use ahma_mcp::{
    adapter::Adapter,
    config::load_tool_configs,
    mcp_service::{AhmaMcpService, GuidanceConfig},
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};
use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use serde_json::{Value, from_str};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{info, instrument};
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

    /// Path to the directory containing tool JSON configuration files.
    #[arg(long, global = true, default_value = "tools")]
    tools_dir: PathBuf,

    /// Path to the tool guidance JSON file.
    #[arg(long, global = true, default_value = "tool_guidance.json")]
    guidance_file: PathBuf,

    /// Default timeout for commands in seconds.
    #[arg(long, global = true, default_value = "300")]
    timeout: u64,

    /// Enable debug logging.
    #[arg(short, long, global = true)]
    debug: bool,

    /// Enable synchronous mode for CLI operations.
    #[arg(long, global = true)]
    synchronous: bool,

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
        tracing::info!("Running in Server mode");
        run_server_mode(cli).await
    } else {
        tracing::info!("Running in CLI mode");
        run_cli_mode(cli).await
    }
}

async fn run_server_mode(cli: Cli) -> Result<()> {
    tracing::info!("Starting ahma_mcp v1.0.0");
    tracing::info!("Tools directory: {:?}", cli.tools_dir);
    tracing::info!("Guidance file: {:?}", cli.guidance_file);
    tracing::info!("Command timeout: {}s", cli.timeout);

    // Load guidance configuration
    let guidance_config = if cli.guidance_file.exists() {
        let guidance_content = fs::read_to_string(&cli.guidance_file)?;
        from_str::<GuidanceConfig>(&guidance_content).ok()
    } else {
        None
    };

    // Initialize the operation monitor
    let monitor_config = MonitorConfig::with_timeout(std::time::Duration::from_secs(cli.timeout));
    let shutdown_timeout = monitor_config.shutdown_timeout; // Clone before moving
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    // Initialize the shell pool manager
    let shell_pool_config = ShellPoolConfig {
        command_timeout: Duration::from_secs(cli.timeout),
        ..Default::default()
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
    shell_pool_manager.clone().start_background_tasks();

    // Initialize the adapter
    let adapter = Arc::new(Adapter::new(operation_monitor.clone(), shell_pool_manager)?);

    // Load tool configurations
    let configs = Arc::new(load_tool_configs(&cli.tools_dir)?);
    if configs.is_empty() {
        tracing::error!("No valid tool configurations found in {:?}", cli.tools_dir);
        // It's not a fatal error to have no tools, just log it.
    } else {
        tracing::info!("Loaded {} tool configurations", configs.len());
    }

    // Create and start the MCP service
    let service_handler = AhmaMcpService::new(
        adapter.clone(),
        operation_monitor.clone(),
        configs,
        Arc::new(guidance_config),
    )
    .await?;
    let service = service_handler.serve(rmcp::transport::stdio()).await?;

    // ============================================================================
    // CRITICAL: Graceful Shutdown Implementation for Development Workflow
    // ============================================================================
    //
    // PURPOSE: Solves "Does the ahma_mcp server shut down gracefully when
    //          .vscode/mcp.json watch triggers a restart?"
    //
    // LESSON LEARNED: cargo watch sends SIGTERM during file changes, causing
    // abrupt termination of ongoing operations. This implementation provides:
    // 1. Signal handling for SIGTERM (cargo watch) and SIGINT (Ctrl+C)
    // 2. 10-second grace period for operations to complete naturally
    // 3. Progress monitoring with user feedback during shutdown
    // 4. Forced exit if service doesn't shutdown within 5 additional seconds
    //
    // DO NOT REMOVE: This is essential for development workflow integration
    // ============================================================================

    // Set up signal handling for graceful shutdown
    let adapter_for_signal = adapter.clone();
    let operation_monitor_for_signal = operation_monitor.clone();
    tokio::spawn(async move {
        // Wait for SIGTERM (sent by cargo watch) or SIGINT (Ctrl+C)
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received SIGINT, initiating graceful shutdown...");
            }
            _ = async {
                #[cfg(unix)]
                {
                    let mut term_signal = signal::unix::signal(signal::unix::SignalKind::terminate())
                        .expect("Failed to setup SIGTERM handler");
                    term_signal.recv().await;
                }
                #[cfg(not(unix))]
                {
                    // On non-Unix systems, just wait indefinitely
                    // The ctrl_c signal above will handle shutdown
                    std::future::pending::<()>().await;
                }
            } => {
                info!("Received SIGTERM (likely from cargo watch), initiating graceful shutdown...");
            }
        }

        // Check for active operations and provide progress feedback
        info!("🛑 Shutdown initiated - checking for active operations...");

        let shutdown_summary = operation_monitor_for_signal.get_shutdown_summary().await;

        if shutdown_summary.total_active > 0 {
            info!(
                "⏳ Waiting up to 15 seconds for {} active operation(s) to complete...",
                shutdown_summary.total_active
            );

            // Wait up to configured timeout for operations to complete with priority-based progress updates
            let shutdown_start = std::time::Instant::now();
            let shutdown_timeout = shutdown_timeout;

            while shutdown_start.elapsed() < shutdown_timeout {
                let current_summary = operation_monitor_for_signal.get_shutdown_summary().await;

                if current_summary.total_active == 0 {
                    info!("✅ All operations completed successfully");
                    break;
                } else if current_summary.total_active != shutdown_summary.total_active {
                    info!(
                        "📈 Progress: {} operations remaining",
                        current_summary.total_active
                    );
                }

                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }

            let final_summary = operation_monitor_for_signal.get_shutdown_summary().await;

            if final_summary.total_active > 0 {
                tracing::warn!(
                    "⚠️  Proceeding with shutdown - {} operation(s) still running",
                    final_summary.total_active
                );
                for op in final_summary.operations.iter() {
                    tracing::warn!("   - {} ({})", op.id, op.tool_name);
                }
            }
        } else {
            info!("✅ No active operations - proceeding with immediate shutdown");
        }

        info!("🔄 Shutting down adapter and shell pools...");
        adapter_for_signal.shutdown().await;

        // Force process exit if service doesn't stop naturally
        tokio::time::sleep(Duration::from_secs(5)).await;
        info!("Service did not stop gracefully, forcing exit");
        std::process::exit(0);
    });

    service.waiting().await?;

    // Gracefully shutdown the adapter
    adapter.shutdown().await;

    Ok(())
}

async fn run_cli_mode(cli: Cli) -> Result<()> {
    let tool_name = cli.tool_name.unwrap(); // Safe due to check in main()

    // Parse tool name (e.g., "cargo_check" -> base_tool: "cargo", subcommand: "check")
    let parts: Vec<&str> = tool_name.split('_').collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid tool name format. Expected 'tool_subcommand'.");
    }
    let base_tool = parts[0];
    let subcommand_name = parts[1..].join("_");

    // Initialize adapter and monitor for CLI mode
    let monitor_config = MonitorConfig::with_timeout(std::time::Duration::from_secs(cli.timeout));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig {
        command_timeout: Duration::from_secs(cli.timeout),
        ..Default::default()
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
    let adapter = Adapter::new(operation_monitor, shell_pool_manager)?;

    // Load tool configurations
    let configs = Arc::new(load_tool_configs(&std::path::PathBuf::from("tools"))?);
    if configs.is_empty() {
        tracing::error!("No valid tool configurations found");
        anyhow::bail!("No tool configurations found");
    }

    // Get the specific tool config
    let config = configs
        .get(base_tool)
        .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found in configurations", base_tool))?;

    // Find the subcommand configuration
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

    // Don't add subcommand name here - it's handled in args_map as "_subcommand"

    // Add predefined args from subcommand config
    // Note: subcommand_config doesn't have args field, using options instead

    let mut working_directory: Option<String> = None;
    let mut tool_args_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Check for environment variable for programmatic execution
    if let Ok(env_args) = std::env::var("AHMA_MCP_ARGS") {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&env_args) {
            if let Some(map) = json_val.as_object() {
                tool_args_map = map.clone();
            }
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

    if working_directory.is_none() {
        if let Some(wd) = tool_args_map
            .get("working_directory")
            .and_then(|v| v.as_str())
        {
            working_directory = Some(wd.to_string());
        }
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

    // Build arguments from subcommand options and user input
    let mut args_map = serde_json::Map::new();

    // Add subcommand as the first positional argument
    args_map.insert(
        "_subcommand".to_string(),
        Value::String(subcommand_name.clone()),
    );

    // Add any additional arguments from command line
    for arg in &raw_args {
        if let Some((key, value)) = arg.split_once('=') {
            args_map.insert(key.to_string(), Value::String(value.to_string()));
        } else {
            // Handle positional arguments or flags
            args_map.insert(arg.clone(), Value::String("".to_string()));
        }
    }

    // Execute the tool
    let result = adapter
        .execute_sync_in_dir(
            base_tool,
            Some(args_map),
            &final_working_dir.unwrap_or_else(|| ".".to_string()),
            subcommand_config.timeout_seconds,
            Some(subcommand_config),
        )
        .await;

    match result {
        Ok(output) => {
            // Print the output directly since execute_sync_in_dir returns a String
            println!("{}", output);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error executing tool: {}", e);
            Err(anyhow::anyhow!("Tool execution failed"))
        }
    }
}
