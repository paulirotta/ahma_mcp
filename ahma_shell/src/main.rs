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

use ahma_core::config::load_tool_configs;
use ahma_core::{
    adapter::Adapter,
    config::{SubcommandConfig, ToolConfig},
    mcp_service::{AhmaMcpService, GuidanceConfig},
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
    tool_availability::evaluate_tool_availability,
    utils::logging::init_logging,
};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use rmcp::ServiceExt;
use serde_json::{from_str, Value};
use std::{
    collections::HashMap,
    fs,
    io::IsTerminal,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::signal;
use tracing::{info, instrument};

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
    #[arg(long, global = true, default_value = ".ahma/tools")]
    tools_dir: PathBuf,

    /// Path to the tool guidance JSON file.
    #[arg(long, global = true, default_value = ".ahma/tool_guidance.json")]
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

    init_logging(log_level, true)?;
    /*
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(format!("ahma_mcp={}", log_level)))
            .with_writer(std::io::stderr)
            .init();
    */

    if cli.server || cli.tool_name.is_none() {
        // Check if stdin is a terminal (interactive mode)
        if std::io::stdin().is_terminal() && !cli.server {
            eprintln!(
                "\n❌ Error: ahma_mcp is an MCP server designed for JSON-RPC communication over stdio.\n"
            );
            eprintln!("It cannot be run directly from an interactive terminal.\n");
            eprintln!("Usage options:");
            eprintln!("  1. Run as MCP server (requires MCP client with stdio transport):");
            eprintln!("     Configure in your MCP client's configuration file\n");
            eprintln!("  2. Execute a single tool command:");
            eprintln!("     ahma_mcp <tool_name> [tool_arguments...]\n");
            eprintln!("For more information, run: ahma_mcp --help\n");
            std::process::exit(1);
        }

        tracing::info!("Running in Server mode");
        run_server_mode(cli).await
    } else {
        tracing::info!("Running in CLI mode");
        run_cli_mode(cli).await
    }
}

fn find_matching_tool<'a>(
    configs: &'a HashMap<String, ToolConfig>,
    tool_name: &str,
) -> Result<(&'a str, &'a ToolConfig)> {
    configs
        .iter()
        .filter(|(_, config)| config.enabled)
        .filter_map(|(key, config)| {
            if tool_name.starts_with(key) {
                Some((key.as_str(), config))
            } else {
                None
            }
        })
        .max_by_key(|(key, _)| key.len())
        .ok_or_else(|| anyhow!("No matching tool configuration found for '{}'", tool_name))
}

fn find_tool_config<'a>(
    configs: &'a HashMap<String, ToolConfig>,
    tool_name: &str,
) -> Option<(&'a str, &'a ToolConfig)> {
    if let Some((key, config)) = configs.get_key_value(tool_name) {
        return Some((key.as_str(), config));
    }

    configs
        .iter()
        .find(|(_, config)| config.name == tool_name)
        .map(|(key, config)| (key.as_str(), config))
}

fn resolve_cli_subcommand<'a>(
    config_key: &str,
    config: &'a ToolConfig,
    tool_name: &str,
    subcommand_override: Option<&str>,
) -> Result<(&'a SubcommandConfig, Vec<String>)> {
    let subcommand_source =
        subcommand_override.unwrap_or_else(|| tool_name.strip_prefix(config_key).unwrap_or(""));
    let trimmed = subcommand_source.trim();
    let is_default_call = trimmed.is_empty() || trimmed == "default";

    let subcommand_parts: Vec<&str> = if is_default_call {
        vec!["default"]
    } else {
        trimmed
            .trim_start_matches('_')
            .split('_')
            .filter(|segment| !segment.is_empty())
            .collect()
    };

    if subcommand_parts.is_empty() {
        anyhow::bail!("Invalid tool name format. Expected 'tool_subcommand'.");
    }

    let mut current_subcommands = config
        .subcommand
        .as_ref()
        .ok_or_else(|| anyhow!("Tool '{}' has no subcommands defined", config_key))?;
    let mut command_parts = vec![config.command.clone()];
    let mut found_subcommand = None;
    let error_path = if is_default_call {
        "default".to_string()
    } else {
        trimmed.trim_start_matches('_').to_string()
    };

    for (index, part) in subcommand_parts.iter().enumerate() {
        if let Some(sub) = current_subcommands
            .iter()
            .find(|candidate| candidate.name == *part && candidate.enabled)
        {
            if sub.name == "default" && is_default_call {
                if subcommand_override.is_none() || trimmed == "default" {
                    let segments: Vec<&str> = config_key.split('_').collect();
                    let derived = if segments.len() > 2 {
                        segments[1..].join("-")
                    } else {
                        segments.last().unwrap_or(&"").to_string()
                    };
                    if !derived.is_empty() && derived != config.command {
                        command_parts.push(derived);
                    }
                }
            } else if sub.name != "default" {
                command_parts.push(sub.name.clone());
            }

            if index == subcommand_parts.len() - 1 {
                found_subcommand = Some(sub);
            } else if let Some(nested) = &sub.subcommand {
                current_subcommands = nested;
            } else {
                anyhow::bail!(
                    "Subcommand '{}' has no nested subcommands for remaining path in tool '{}'",
                    error_path,
                    config_key
                );
            }
        } else {
            anyhow::bail!(
                "Subcommand '{}' not found for tool '{}'",
                error_path,
                config_key
            );
        }
    }

    let subcommand_config = found_subcommand.ok_or_else(|| {
        anyhow!(
            "Subcommand '{}' not found for tool '{}'",
            error_path,
            config_key
        )
    })?;

    Ok((subcommand_config, command_parts))
}

async fn run_cli_sequence(
    adapter: &Adapter,
    configs: &HashMap<String, ToolConfig>,
    parent_config: &ToolConfig,
    subcommand_config: &SubcommandConfig,
    working_dir: &str,
) -> Result<()> {
    let sequence = subcommand_config
        .sequence
        .as_ref()
        .ok_or_else(|| anyhow!("Sequence not defined for tool '{}'", parent_config.name))?;

    let delay_ms = subcommand_config
        .step_delay_ms
        .or(parent_config.step_delay_ms)
        .unwrap_or(0);

    for (index, step) in sequence.iter().enumerate() {
        let (step_key, step_tool_config) = find_tool_config(configs, &step.tool)
            .ok_or_else(|| anyhow!("Sequence step tool '{}' not found", step.tool))?;

        let (step_subcommand_config, command_parts) =
            resolve_cli_subcommand(step_key, step_tool_config, step_key, Some(&step.subcommand))?;

        println!(
            "▶ Running sequence step {} ({} {}):",
            index + 1,
            step.tool,
            step.subcommand
        );

        let output = adapter
            .execute_sync_in_dir(
                &command_parts.join(" "),
                Some(step.args.clone()),
                working_dir,
                step_subcommand_config.timeout_seconds,
                Some(step_subcommand_config),
            )
            .await
            .with_context(|| format!("Sequence step '{} {}' failed", step.tool, step.subcommand))?;

        if !output.trim().is_empty() {
            println!("{}", output);
        } else {
            println!("✓ Completed without output");
        }

        if delay_ms > 0 && index + 1 < sequence.len() {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    Ok(())
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
    let adapter = Arc::new(Adapter::new(
        operation_monitor.clone(),
        shell_pool_manager.clone(),
    )?);

    // Load tool configurations
    let raw_configs = load_tool_configs(&cli.tools_dir)?;
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let availability_summary = evaluate_tool_availability(
        shell_pool_manager.clone(),
        raw_configs,
        working_dir.as_path(),
    )
    .await?;

    if !availability_summary.disabled_tools.is_empty() {
        for disabled in &availability_summary.disabled_tools {
            tracing::warn!(
                "Tool '{}' disabled at startup. {}",
                disabled.name,
                disabled.message
            );
            if let Some(instructions) = &disabled.install_instructions {
                tracing::info!(
                    "Install instructions for '{}': {}",
                    disabled.name,
                    instructions
                );
            }
        }
    }

    if !availability_summary.disabled_subcommands.is_empty() {
        for disabled in &availability_summary.disabled_subcommands {
            tracing::warn!(
                "Tool subcommand '{}::{}' disabled at startup. {}",
                disabled.tool,
                disabled.subcommand_path,
                disabled.message
            );
            if let Some(instructions) = &disabled.install_instructions {
                tracing::info!(
                    "Install instructions for '{}::{}': {}",
                    disabled.tool,
                    disabled.subcommand_path,
                    instructions
                );
            }
        }
    }

    let configs = Arc::new(availability_summary.filtered_configs);
    if configs.is_empty() {
        tracing::error!("No valid tool configurations available after availability checks");
        tracing::error!("Tools directory: {:?}", cli.tools_dir);
        // It's not a fatal error to have no tools, just log it.
    } else {
        let tool_names: Vec<String> = configs.keys().cloned().collect();
        tracing::info!(
            "Loaded {} tool configurations ({} disabled): {}",
            configs.len(),
            availability_summary.disabled_tools.len(),
            tool_names.join(", ")
        );
    }

    // Create and start the MCP service
    let service_handler = AhmaMcpService::new(
        adapter.clone(),
        operation_monitor.clone(),
        configs,
        Arc::new(guidance_config),
        cli.synchronous,
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
    // 2. 360-second grace period for operations to complete naturally
    // 3. Progress monitoring with user feedback during shutdown
    // 4. Forced exit if service doesn't shutdown within 5 additional seconds
    //
    // DO NOT REMOVE: This is essential for development workflow integration
    // ============================================================================

    // Set up signal handling for graceful shutdown
    let adapter_for_signal = adapter.clone();
    let operation_monitor_for_signal = operation_monitor.clone();
    tokio::spawn(async move {
        let shutdown_reason = tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received SIGINT, initiating graceful shutdown...");
                "Cancelled due to SIGINT (Ctrl+C) - user interrupt"
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
                    // On non-Unix systems, just await indefinitely
                    // The ctrl_c signal above will handle shutdown
                    std::future::pending::<()>().await;
                }
            } => {
                info!("Received SIGTERM (likely from cargo watch), initiating graceful shutdown...");
                "Cancelled due to SIGTERM from cargo watch - source code reload"
            }
        };

        // Check for active operations and provide progress feedback
        info!("🛑 Shutdown initiated - checking for active operations...");

        let shutdown_summary = operation_monitor_for_signal.get_shutdown_summary().await;

        if shutdown_summary.total_active > 0 {
            info!(
                "⏳ Waiting up to 15 seconds for {} active operation(s) to complete...",
                shutdown_summary.total_active
            );

            // Wait up to configured timeout for operations to complete with priority-based progress updates
            let shutdown_start = Instant::now();
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
                info!(
                    "⏱️  Shutdown timeout reached - cancelling {} remaining operation(s) with reason: {}",
                    final_summary.total_active, shutdown_reason
                );

                // Cancel remaining operations with descriptive reason
                for op in final_summary.operations.iter() {
                    tracing::debug!(
                        "Attempting to cancel operation '{}' ({}) with reason: '{}'",
                        op.id,
                        op.tool_name,
                        shutdown_reason
                    );

                    let cancelled = operation_monitor_for_signal
                        .cancel_operation_with_reason(&op.id, Some(shutdown_reason.to_string()))
                        .await;

                    if cancelled {
                        info!("   ✓ Cancelled operation '{}' ({})", op.id, op.tool_name);
                        tracing::debug!("Successfully cancelled operation '{}' with reason", op.id);
                    } else {
                        tracing::warn!(
                            "   ⚠ Failed to cancel operation '{}' ({})",
                            op.id,
                            op.tool_name
                        );
                        tracing::debug!(
                            "Failed to cancel operation '{}' - it may have already completed",
                            op.id
                        );
                    }
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
    let tool_name = cli.tool_name.unwrap();

    // Initialize adapter and monitor for CLI mode
    let monitor_config = MonitorConfig::with_timeout(std::time::Duration::from_secs(cli.timeout));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig {
        command_timeout: Duration::from_secs(cli.timeout),
        ..Default::default()
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));
    let adapter = Adapter::new(operation_monitor, shell_pool_manager.clone())?;

    // Load tool configurations
    let raw_configs = load_tool_configs(&PathBuf::from(".ahma/tools"))?;
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let availability_summary = evaluate_tool_availability(
        shell_pool_manager.clone(),
        raw_configs,
        working_dir.as_path(),
    )
    .await?;

    if !availability_summary.disabled_tools.is_empty() {
        for disabled in &availability_summary.disabled_tools {
            tracing::warn!(
                "Tool '{}' disabled at CLI startup. {}",
                disabled.name,
                disabled.message
            );
        }
    }

    let configs = Arc::new(availability_summary.filtered_configs);
    if configs.is_empty() {
        tracing::error!("No valid tool configurations available after availability checks");
        anyhow::bail!("No tool configurations found");
    }

    let configs_ref = configs.as_ref();
    let (config_key, config) = find_matching_tool(configs_ref, &tool_name)?;
    let (subcommand_config, command_parts) =
        resolve_cli_subcommand(config_key, config, &tool_name, None)?;

    let mut raw_args = Vec::new();
    let mut working_directory: Option<String> = None;
    let mut tool_args_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Prefer programmatic arguments via environment variable
    if let Ok(env_args) = std::env::var("AHMA_MCP_ARGS") {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&env_args) {
            if let Some(map) = json_val.as_object() {
                tool_args_map = map.clone();
            }
        }
    } else {
        let mut iter = cli.tool_args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                raw_args.extend(iter.map(|s| s.to_string()));
                break;
            }

            if arg.starts_with("--") {
                let key = arg.trim_start_matches("--").to_string();
                if let Some(next) = iter.peek() {
                    if next.starts_with('-') {
                        tool_args_map.insert(key, serde_json::Value::Bool(true));
                    } else if let Some(val) = iter.next() {
                        if key == "working-directory" {
                            working_directory = Some(val);
                        } else {
                            tool_args_map.insert(key, serde_json::Value::String(val));
                        }
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
    let working_dir_str = final_working_dir.unwrap_or_else(|| ".".to_string());

    let mut args_map = serde_json::Map::new();
    if raw_args.first().map(|s| s.as_str()) == Some("default") {
        raw_args.remove(0);
    }

    for (k, v) in tool_args_map.iter() {
        args_map.insert(k.clone(), v.clone());
    }

    for arg in &raw_args {
        if let Some((key, value)) = arg.split_once('=') {
            args_map.insert(key.to_string(), Value::String(value.to_string()));
        } else {
            args_map.insert(arg.clone(), Value::String(String::new()));
        }
    }

    if config.command == "sequence" && subcommand_config.sequence.is_some() {
        run_cli_sequence(
            &adapter,
            configs_ref,
            config,
            subcommand_config,
            &working_dir_str,
        )
        .await?;
        return Ok(());
    }

    let base_command = command_parts.join(" ");

    let result = adapter
        .execute_sync_in_dir(
            &base_command,
            Some(args_map),
            &working_dir_str,
            subcommand_config.timeout_seconds,
            Some(subcommand_config),
        )
        .await;

    match result {
        Ok(output) => {
            println!("{}", output);
            Ok(())
        }
        Err(e) => {
            let error_message = e.to_string();
            if error_message.contains("Canceled: Canceled") {
                eprintln!(
                    "Operation cancelled by user request (was: {})",
                    error_message
                );
            } else if error_message.contains("task cancelled for reason") {
                eprintln!(
                    "Operation cancelled by user request or system signal (detected MCP cancellation)"
                );
            } else if error_message.to_lowercase().contains("cancel") {
                eprintln!("Operation cancelled: {}", error_message);
            } else {
                eprintln!("Error executing tool: {}", e);
            }
            Err(anyhow::anyhow!("Tool execution failed"))
        }
    }
}
