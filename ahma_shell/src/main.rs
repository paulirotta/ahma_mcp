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
    sandbox::{self, SandboxError},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
    tool_availability::{evaluate_tool_availability, format_install_guidance},
    utils::logging::init_logging,
};
use ahma_http_mcp_client::client::HttpMcpTransport;
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use rmcp::ServiceExt;
use serde_json::{Value, from_str};
use std::{
    collections::{HashMap, HashSet},
    env,
    io::IsTerminal,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{fs, signal};
use tracing::{info, instrument};

mod list_tools;

/// Ahma MCP Server: A generic, config-driven adapter for CLI tools.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "ahma_mcp runs in four modes:

1. STDIO Mode (default): MCP server over stdio for direct integration.
   Example: ahma_mcp --mode stdio

2. HTTP Mode: HTTP bridge server that proxies to stdio MCP server.
   Example: ahma_mcp --mode http --http-port 3000

3. CLI Mode: Execute a single command and print result to stdout.
   Example: ahma_mcp cargo_build --working-directory . -- --release

4. List Tools Mode: List all tools from an MCP server.
   Example: ahma_mcp --list-tools -- /path/to/server
   Example: ahma_mcp --list-tools --http http://localhost:3000"
)]
struct Cli {
    /// List all tools from an MCP server and exit
    #[arg(long)]
    list_tools: bool,

    /// Name of the server in mcp.json to connect to (for --list-tools mode)
    #[arg(long)]
    server: Option<String>,

    /// HTTP URL of the MCP server to list tools from (for --list-tools mode)
    #[arg(long)]
    http: Option<String>,

    /// Output format for --list-tools: text (default) or json
    #[arg(long, default_value = "text")]
    format: list_tools::OutputFormat,

    /// Server mode: 'stdio' (default) or 'http'
    #[arg(long, default_value = "stdio", value_parser = ["stdio", "http"])]
    mode: String,

    /// HTTP server port (only used in http mode)
    #[arg(long, default_value = "3000")]
    http_port: u16,

    /// HTTP server host (only used in http mode)
    #[arg(long, default_value = "127.0.0.1")]
    http_host: String,

    /// Enable session isolation mode for HTTP bridge.
    /// Each client gets a separate subprocess with its own sandbox scope
    /// derived from the client's workspace roots.
    #[arg(long)]
    session_isolation: bool,

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

    /// Force synchronous mode for all operations (overrides default asynchronous behavior).
    /// By default, tools run asynchronously (non-blocking). Use this flag to force all tools
    /// to run synchronously (blocking until complete).
    #[arg(long, global = true)]
    sync: bool,

    /// Override the sandbox scope (root directory for file system operations).
    /// By default, uses the current working directory for stdio mode.
    /// Can be specified multiple times for multi-root workspaces.
    #[arg(long, global = true)]
    sandbox_scope: Vec<PathBuf>,

    /// Disable Ahma's kernel-level sandboxing (sandbox-exec on macOS, Landlock on Linux).
    /// Use this when running inside another sandbox (e.g., Cursor, VS Code, Docker) where
    /// nested sandboxing causes "Operation not permitted" errors. The outer sandbox still
    /// provides security. Can also be set via AHMA_NO_SANDBOX=1 environment variable.
    #[arg(long, global = true)]
    no_sandbox: bool,

    /// Defer sandbox initialization until the client provides workspace roots via roots/list.
    /// Used by HTTP bridge to allow clients to specify their own workspace scope.
    /// The sandbox will be initialized from the roots/list response instead of cwd.
    #[arg(long, global = true)]
    defer_sandbox: bool,

    /// Log to stderr instead of file (useful for debugging and seeing errors in terminal).
    /// Enables colored output on Mac/Linux.
    #[arg(long, global = true)]
    log_to_stderr: bool,

    /// The name of the tool to execute (e.g., 'cargo_build').
    #[arg()]
    tool_name: Option<String>,

    /// Path to the mcp.json file for client configurations.
    #[arg(long, global = true, default_value = ".vscode/mcp.json")]
    mcp_config: PathBuf,

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
    let log_to_file = !cli.log_to_stderr;

    init_logging(log_level, log_to_file)?;

    // Handle --list-tools mode early (before sandbox initialization)
    // This mode doesn't execute tools locally, so it doesn't need sandbox
    if cli.list_tools {
        tracing::info!("Running in list-tools mode");
        return run_list_tools_mode(&cli).await;
    }

    // Check if sandbox should be disabled
    // Can be set via --no-sandbox flag, AHMA_NO_SANDBOX=1, or AHMA_TEST_MODE (legacy)
    let mut no_sandbox = cli.no_sandbox
        || std::env::var("AHMA_NO_SANDBOX").is_ok()
        || std::env::var("AHMA_TEST_MODE").is_ok();

    if no_sandbox {
        tracing::warn!("Ahma sandbox disabled via --no-sandbox flag or environment variable");
        sandbox::enable_test_mode();
    } else {
        // Check sandbox prerequisites before anything else
        if let Err(e) = sandbox::check_sandbox_prerequisites() {
            sandbox::exit_with_sandbox_error(&e);
        }

        // On macOS, test if sandbox-exec can actually be applied
        // This detects when running inside another sandbox (Cursor, VS Code, Docker)
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = sandbox::test_sandbox_exec_available() {
                match e {
                    SandboxError::NestedSandboxDetected => {
                        tracing::warn!(
                            "Nested sandbox detected - Ahma is running inside another sandbox (e.g., Cursor IDE)"
                        );
                        tracing::warn!(
                            "Ahma's sandbox will be disabled; the outer sandbox provides security"
                        );
                        tracing::info!(
                            "To suppress this warning, use --no-sandbox or set AHMA_NO_SANDBOX=1"
                        );
                        sandbox::enable_test_mode();
                        no_sandbox = true;
                    }
                    _ => {
                        // Other sandbox errors should be fatal
                        sandbox::exit_with_sandbox_error(&e);
                    }
                }
            }
        }
    }

    // Initialize sandbox scope(s)
    // Priority: 1. CLI --sandbox-scope, 2. AHMA_SANDBOX_SCOPE env var, 3. current working directory
    let sandbox_scopes: Vec<PathBuf> = if !cli.sandbox_scope.is_empty() {
        // CLI override takes precedence - canonicalize each scope
        let mut scopes = Vec::with_capacity(cli.sandbox_scope.len());
        for scope in &cli.sandbox_scope {
            let canonical = std::fs::canonicalize(scope)
                .with_context(|| format!("Failed to canonicalize sandbox scope: {:?}", scope))?;
            scopes.push(canonical);
        }
        scopes
    } else if let Ok(env_scope) = std::env::var("AHMA_SANDBOX_SCOPE") {
        // Environment variable is second priority (comma-separated for multiple roots)
        let mut scopes = Vec::new();
        for scope_str in env_scope.split(',') {
            let scope_str = scope_str.trim();
            if !scope_str.is_empty() {
                let env_path = PathBuf::from(scope_str);
                let canonical = std::fs::canonicalize(&env_path).with_context(|| {
                    format!(
                        "Failed to canonicalize AHMA_SANDBOX_SCOPE path: {:?}",
                        scope_str
                    )
                })?;
                scopes.push(canonical);
            }
        }
        if scopes.is_empty() {
            vec![
                std::env::current_dir()
                    .context("Failed to get current working directory for sandbox scope")?,
            ]
        } else {
            scopes
        }
    } else {
        // Default to current working directory
        vec![
            std::env::current_dir()
                .context("Failed to get current working directory for sandbox scope")?,
        ]
    };

    // Skip sandbox initialization if deferred (will be set via roots/list)
    if cli.defer_sandbox {
        tracing::info!("Sandbox initialization deferred - will be set from client roots/list");
    } else {
        // Initialize the global sandbox scope(s) (can only be done once)
        if let Err(e) = sandbox::initialize_sandbox_scopes(&sandbox_scopes) {
            match e {
                SandboxError::AlreadyInitialized => {
                    // This shouldn't happen in normal operation, but handle gracefully
                    tracing::warn!("Sandbox scope was already initialized");
                }
                _ => {
                    return Err(anyhow!("Failed to initialize sandbox scope: {}", e));
                }
            }
        }
        tracing::info!("Sandbox scope(s) initialized: {:?}", sandbox_scopes);
    }

    // Apply kernel-level sandbox restrictions on Linux (skip if disabled or deferred)
    // Note: Landlock only supports a single root, so we use the first scope
    #[cfg(target_os = "linux")]
    if !no_sandbox && !cli.defer_sandbox {
        if let Some(first_scope) = sandbox_scopes.first() {
            if let Err(e) = sandbox::enforce_landlock_sandbox(first_scope) {
                tracing::error!("Failed to enforce Landlock sandbox: {}", e);
                return Err(e);
            }
        }
    }

    // Log the active sandbox mode for clarity
    if no_sandbox {
        tracing::info!("🔓 Sandbox mode: DISABLED (commands run without Ahma sandboxing)");
    } else {
        #[cfg(target_os = "linux")]
        tracing::info!("🔒 Sandbox mode: LANDLOCK (Linux kernel-level file system restrictions)");
        #[cfg(target_os = "macos")]
        tracing::info!("🔒 Sandbox mode: SEATBELT (macOS sandbox-exec per-command restrictions)");
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        tracing::info!("🔒 Sandbox mode: ACTIVE");
    }

    // Determine mode based on CLI arguments
    let is_server_mode = cli.tool_name.is_none();

    if is_server_mode {
        match cli.mode.as_str() {
            "http" => {
                tracing::info!("Running in HTTP bridge mode");
                run_http_bridge_mode(cli).await
            }
            "stdio" => {
                // Check if stdin is a terminal (interactive mode)
                if std::io::stdin().is_terminal() {
                    eprintln!(
                        "\n❌ Error: ahma_mcp is an MCP server designed for JSON-RPC communication over stdio.\n"
                    );
                    eprintln!("It cannot be run directly from an interactive terminal.\n");
                    eprintln!("Usage options:");
                    eprintln!("  1. Run as stdio MCP server (requires MCP client):");
                    eprintln!("     ahma_mcp --mode stdio\n");
                    eprintln!("  2. Run as HTTP bridge server:");
                    eprintln!("     ahma_mcp --mode http --http-port 3000\n");
                    eprintln!("  3. Execute a single tool command:");
                    eprintln!("     ahma_mcp <tool_name> [tool_arguments...]\n");
                    eprintln!("For more information, run: ahma_mcp --help\n");
                    std::process::exit(1);
                }

                tracing::info!("Running in STDIO server mode");
                run_server_mode(cli).await
            }
            _ => {
                eprintln!("Invalid mode: {}. Use 'stdio' or 'http'", cli.mode);
                std::process::exit(1);
            }
        }
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
                // Logic to derive subcommand from tool name (e.g. cargo_build -> cargo build)
                // is removed because it causes issues for tools like bash (bash -c async).
                // If a tool needs a subcommand, it should be explicit in the config or the command.
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

    let skip_tools = parse_env_list("AHMA_SKIP_SEQUENCE_TOOLS");
    let skip_subcommands = parse_env_list("AHMA_SKIP_SEQUENCE_SUBCOMMANDS");

    for (index, step) in sequence.iter().enumerate() {
        if should_skip(&skip_tools, &step.tool) {
            println!(
                "Skipping sequence step {} ({} {}) due to environment override.",
                index + 1,
                step.tool,
                step.subcommand
            );
            continue;
        }

        if should_skip(&skip_subcommands, &step.subcommand) {
            println!(
                "Skipping sequence step {} ({} {}) due to environment override.",
                index + 1,
                step.tool,
                step.subcommand
            );
            continue;
        }

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

fn parse_env_list(key: &str) -> Option<HashSet<String>> {
    env::var(key).ok().map(|list| {
        list.split(',')
            .map(|entry| entry.trim().to_ascii_lowercase())
            .filter(|entry| !entry.is_empty())
            .collect()
    })
}

fn should_skip(set: &Option<HashSet<String>>, value: &str) -> bool {
    set.as_ref()
        .map(|items| items.contains(&value.to_ascii_lowercase()))
        .unwrap_or(false)
}

/// Run in list-tools mode: connect to an MCP server and list all available tools
async fn run_list_tools_mode(cli: &Cli) -> Result<()> {
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

        list_tools::list_tools_stdio(&command_args).await?
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

async fn run_http_bridge_mode(cli: Cli) -> Result<()> {
    use ahma_http_bridge::{BridgeConfig, start_bridge};

    let bind_addr = format!("{}:{}", cli.http_host, cli.http_port)
        .parse()
        .context("Invalid HTTP host/port")?;

    tracing::info!("Starting HTTP bridge on {}", bind_addr);
    if cli.session_isolation {
        tracing::info!("Session isolation mode ENABLED - each client gets a separate subprocess");
    }

    // Build the command to run the stdio MCP server
    let server_command = env::current_exe()
        .context("Failed to get current executable path")?
        .to_string_lossy()
        .to_string();

    // Get the sandbox scope that was initialized in main()
    // This ensures the subprocess uses the same sandbox as the parent
    let sandbox_scope = sandbox::get_sandbox_scope()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });

    // In session isolation mode, we don't pass --sandbox-scope to subprocess args
    // because each session will have its own scope derived from roots/list
    let mut server_args = vec![
        "--mode".to_string(),
        "stdio".to_string(),
        "--tools-dir".to_string(),
        cli.tools_dir.to_string_lossy().to_string(),
        "--guidance-file".to_string(),
        cli.guidance_file.to_string_lossy().to_string(),
        "--timeout".to_string(),
        cli.timeout.to_string(),
    ];

    // Only add sandbox-scope for non-session-isolation mode
    if !cli.session_isolation {
        server_args.push("--sandbox-scope".to_string());
        server_args.push(sandbox_scope.clone());
    }

    if cli.debug {
        server_args.push("--debug".to_string());
    }

    if cli.sync {
        server_args.push("--sync".to_string());
    }

    // Enable colored output for HTTP mode (shows STDIN/STDOUT/STDERR debug info)
    // Per R8B, this is always enabled for HTTP mode to aid debugging
    let enable_colored_output = true;
    tracing::info!(
        "HTTP bridge mode - colored terminal output enabled (v{})",
        env!("CARGO_PKG_VERSION")
    );
    if !cli.session_isolation {
        tracing::info!(
            "HTTP subprocess sandbox scope: {:?}",
            sandbox::get_sandbox_scopes()
        );
    }

    // Get first sandbox scope for default (backwards compatibility)
    let default_scope = sandbox::get_sandbox_scope()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let config = BridgeConfig {
        bind_addr,
        server_command,
        server_args,
        enable_colored_output,
        session_isolation: cli.session_isolation,
        default_sandbox_scope: default_scope,
    };

    start_bridge(config).await?;

    Ok(())
}

async fn run_server_mode(cli: Cli) -> Result<()> {
    tracing::info!("Starting ahma_mcp v1.0.0");
    tracing::info!("Tools directory: {:?}", cli.tools_dir);
    tracing::info!("Guidance file: {:?}", cli.guidance_file);
    tracing::info!("Command timeout: {}s", cli.timeout);

    // --- MCP Client Mode ---
    if fs::try_exists(&cli.mcp_config).await.unwrap_or(false) {
        // Try to load the MCP config, but ignore if it's not a valid ahma_mcp config
        // (e.g., if it's a Cursor/VSCode MCP server config with "type": "stdio")
        match ahma_core::config::load_mcp_config(&cli.mcp_config).await {
            Ok(mcp_config) => {
                if let Some(server_config) = mcp_config.servers.values().next()
                    && let ahma_core::config::ServerConfig::Http(http_config) = server_config
                {
                    tracing::info!("Initializing HTTP MCP Client for: {}", http_config.url);

                    let url = url::Url::parse(&http_config.url)
                        .context("Failed to parse MCP server URL")?;

                    let transport = HttpMcpTransport::new(
                        url,
                        http_config.atlassian_client_id.clone(),
                        http_config.atlassian_client_secret.clone(),
                    )?;

                    // Authenticate if needed
                    transport.ensure_authenticated().await?;

                    tracing::info!("Successfully connected to HTTP MCP server");
                    tracing::warn!(
                        "Remote tools are not yet proxied to the client - this is a partial integration"
                    );

                    // Keep the transport alive for the duration of the process
                    // This ensures the background SSE listener continues to run
                    Box::leak(Box::new(transport));
                }
            }
            Err(e) => {
                // Ignore config parse errors - the file might be a Cursor/VSCode MCP config
                tracing::debug!(
                    "Could not parse mcp.json as ahma_mcp config (this is OK if it's a Cursor/VSCode MCP config): {}",
                    e
                );
            }
        }
    }

    // --- Standard Server Mode ---
    tracing::info!("Running in standard child-process server mode.");

    // Load guidance configuration (using async I/O)
    let guidance_config = if fs::try_exists(&cli.guidance_file).await.unwrap_or(false) {
        let guidance_content = fs::read_to_string(&cli.guidance_file).await?;
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

    // Load tool configurations (now async, no spawn_blocking needed)
    let tools_dir = cli.tools_dir.clone();
    let raw_configs = load_tool_configs(&tools_dir)
        .await
        .context("Failed to load tool configurations")?;
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

    if !availability_summary.disabled_tools.is_empty()
        || !availability_summary.disabled_subcommands.is_empty()
    {
        let install_guidance = format_install_guidance(&availability_summary);
        tracing::warn!(
            "Startup tool guidance (share with users who need to install prerequisites):\n{}",
            install_guidance
        );
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
    // With async-by-default, we pass force_synchronous=true when --sync flag is used
    let force_synchronous = cli.sync;
    let service_handler = AhmaMcpService::new(
        adapter.clone(),
        operation_monitor.clone(),
        configs,
        Arc::new(guidance_config),
        force_synchronous,
    )
    .await?;

    // Start the config watcher to support hot-reloading of tools
    service_handler.start_config_watcher(cli.tools_dir.clone());

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

    // Load tool configurations (now async, no spawn_blocking needed)
    let raw_configs = load_tool_configs(&cli.tools_dir)
        .await
        .context("Failed to load tool configurations")?;
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

    // Check if this is a top-level sequence tool (no subcommands, just sequence)
    let is_top_level_sequence = config.command == "sequence" && config.sequence.is_some();

    let (subcommand_config, command_parts) = if is_top_level_sequence {
        // For top-level sequence tools, create a dummy subcommand config
        // The actual sequence execution happens later
        let dummy_subcommand = SubcommandConfig {
            name: config.name.clone(),
            description: config.description.clone(),
            subcommand: None,
            options: None,
            positional_args: None,
            positional_args_first: None,
            timeout_seconds: config.timeout_seconds,
            synchronous: config.synchronous,
            enabled: true,
            guidance_key: config.guidance_key.clone(),
            sequence: config.sequence.clone(),
            step_delay_ms: config.step_delay_ms,
            availability_check: None,
            install_instructions: None,
        };
        (
            Box::leak(Box::new(dummy_subcommand)) as &SubcommandConfig,
            vec![config.command.clone()],
        )
    } else {
        let (sub, parts) = resolve_cli_subcommand(config_key, config, &tool_name, None)?;
        (sub, parts)
    };

    let mut raw_args = Vec::new();
    let mut working_directory: Option<String> = None;
    let mut tool_args_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Prefer programmatic arguments via environment variable
    if let Ok(env_args) = std::env::var("AHMA_MCP_ARGS") {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&env_args)
            && let Some(map) = json_val.as_object()
        {
            tool_args_map = map.clone();
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
    let working_dir_str = final_working_dir.unwrap_or_else(|| ".".to_string());

    let mut args_map = serde_json::Map::new();
    if raw_args.first().map(|s| s.as_str()) == Some("default") {
        raw_args.remove(0);
    }

    for (k, v) in tool_args_map.iter() {
        args_map.insert(k.clone(), v.clone());
    }

    let mut positional_iter = subcommand_config
        .positional_args
        .as_ref()
        .map(|v| v.iter())
        .unwrap_or_else(|| [].iter());

    for arg in &raw_args {
        if let Some((key, value)) = arg.split_once('=') {
            args_map.insert(key.to_string(), Value::String(value.to_string()));
        } else {
            // Try to map to next positional arg
            if let Some(pos_arg) = positional_iter.next() {
                args_map.insert(pos_arg.name.clone(), Value::String(arg.clone()));
            } else {
                // Fallback: treat as key with empty value (old behavior)
                args_map.insert(arg.clone(), Value::String(String::new()));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ahma_core::config::{SubcommandConfig, ToolConfig, ToolHints};
    use std::collections::{HashMap, HashSet};

    // =========================================================================
    // Helper: Create a minimal ToolConfig for testing
    // =========================================================================
    fn make_tool_config(name: &str, command: &str, enabled: bool) -> ToolConfig {
        ToolConfig {
            name: name.to_string(),
            description: format!("{} tool", name),
            command: command.to_string(),
            enabled,
            timeout_seconds: None,
            synchronous: None,
            guidance_key: None,
            input_schema: None,
            hints: ToolHints::default(),
            subcommand: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        }
    }

    fn make_tool_config_with_subcommands(
        name: &str,
        command: &str,
        enabled: bool,
        subcommands: Vec<SubcommandConfig>,
    ) -> ToolConfig {
        ToolConfig {
            name: name.to_string(),
            description: format!("{} tool", name),
            command: command.to_string(),
            enabled,
            timeout_seconds: None,
            synchronous: None,
            guidance_key: None,
            input_schema: None,
            hints: ToolHints::default(),
            subcommand: Some(subcommands),
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        }
    }

    fn make_subcommand(name: &str, enabled: bool) -> SubcommandConfig {
        SubcommandConfig {
            name: name.to_string(),
            description: format!("{} subcommand", name),
            subcommand: None,
            options: None,
            positional_args: None,
            positional_args_first: None,
            timeout_seconds: None,
            synchronous: None,
            enabled,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        }
    }

    fn make_subcommand_with_nested(
        name: &str,
        enabled: bool,
        nested: Vec<SubcommandConfig>,
    ) -> SubcommandConfig {
        SubcommandConfig {
            name: name.to_string(),
            description: format!("{} subcommand", name),
            subcommand: Some(nested),
            options: None,
            positional_args: None,
            positional_args_first: None,
            timeout_seconds: None,
            synchronous: None,
            enabled,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        }
    }

    // =========================================================================
    // Tests for: find_matching_tool
    // =========================================================================
    mod find_matching_tool_tests {
        use super::*;

        #[test]
        fn returns_exact_match_when_tool_name_matches_key() {
            let mut configs = HashMap::new();
            configs.insert(
                "cargo".to_string(),
                make_tool_config("cargo", "cargo", true),
            );

            let result = find_matching_tool(&configs, "cargo").unwrap();

            assert_eq!(result.0, "cargo");
            assert_eq!(result.1.name, "cargo");
        }

        #[test]
        fn returns_longest_prefix_match_when_multiple_tools_match() {
            let mut configs = HashMap::new();
            configs.insert(
                "cargo".to_string(),
                make_tool_config("cargo", "cargo", true),
            );
            configs.insert(
                "cargo_build".to_string(),
                make_tool_config("cargo_build", "cargo build", true),
            );

            let result = find_matching_tool(&configs, "cargo_build_release").unwrap();

            assert_eq!(result.0, "cargo_build");
        }

        #[test]
        fn ignores_disabled_tools() {
            let mut configs = HashMap::new();
            configs.insert(
                "cargo_build".to_string(),
                make_tool_config("cargo_build", "cargo build", false),
            );
            configs.insert(
                "cargo".to_string(),
                make_tool_config("cargo", "cargo", true),
            );

            let result = find_matching_tool(&configs, "cargo_build").unwrap();

            // Should match "cargo" since "cargo_build" is disabled
            assert_eq!(result.0, "cargo");
        }

        #[test]
        fn returns_error_when_no_tool_matches() {
            let mut configs = HashMap::new();
            configs.insert(
                "cargo".to_string(),
                make_tool_config("cargo", "cargo", true),
            );

            let result = find_matching_tool(&configs, "rustc");

            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("No matching tool"));
        }

        #[test]
        fn returns_error_when_all_matching_tools_disabled() {
            let mut configs = HashMap::new();
            configs.insert(
                "cargo".to_string(),
                make_tool_config("cargo", "cargo", false),
            );

            let result = find_matching_tool(&configs, "cargo_test");

            assert!(result.is_err());
        }
    }

    // =========================================================================
    // Tests for: find_tool_config
    // =========================================================================
    mod find_tool_config_tests {
        use super::*;

        #[test]
        fn finds_tool_by_key() {
            let mut configs = HashMap::new();
            configs.insert(
                "cargo".to_string(),
                make_tool_config("cargo", "cargo", true),
            );

            let result = find_tool_config(&configs, "cargo");

            assert!(result.is_some());
            let (key, config) = result.unwrap();
            assert_eq!(key, "cargo");
            assert_eq!(config.name, "cargo");
        }

        #[test]
        fn finds_tool_by_name_when_key_differs() {
            let mut configs = HashMap::new();
            let mut tool = make_tool_config("cargo-tool", "cargo", true);
            tool.name = "cargo-tool".to_string();
            configs.insert("cargo_alias".to_string(), tool);

            let result = find_tool_config(&configs, "cargo-tool");

            assert!(result.is_some());
            let (key, config) = result.unwrap();
            assert_eq!(key, "cargo_alias");
            assert_eq!(config.name, "cargo-tool");
        }

        #[test]
        fn returns_none_when_tool_not_found() {
            let configs: HashMap<String, ToolConfig> = HashMap::new();

            let result = find_tool_config(&configs, "nonexistent");

            assert!(result.is_none());
        }

        #[test]
        fn prefers_exact_key_match_over_name_match() {
            let mut configs = HashMap::new();
            let mut tool1 = make_tool_config("other_name", "cmd1", true);
            tool1.name = "cargo".to_string();
            configs.insert("alias".to_string(), tool1);

            let mut tool2 = make_tool_config("cargo", "cmd2", true);
            tool2.name = "different_name".to_string();
            configs.insert("cargo".to_string(), tool2);

            let result = find_tool_config(&configs, "cargo");

            assert!(result.is_some());
            let (key, _) = result.unwrap();
            assert_eq!(key, "cargo"); // Key match should win
        }
    }

    // =========================================================================
    // Tests for: parse_env_list
    // =========================================================================
    mod parse_env_list_tests {
        use super::*;

        #[test]
        fn returns_none_when_env_var_not_set() {
            // Use a unique env var name unlikely to exist
            // SAFETY: Test runs in isolation, env var manipulation is safe
            unsafe { env::remove_var("AHMA_TEST_PARSE_ENV_LIST_UNSET") };

            let result = parse_env_list("AHMA_TEST_PARSE_ENV_LIST_UNSET");

            assert!(result.is_none());
        }

        #[test]
        fn parses_single_item() {
            // SAFETY: Test runs in isolation, env var manipulation is safe
            unsafe { env::set_var("AHMA_TEST_SINGLE", "item1") };

            let result = parse_env_list("AHMA_TEST_SINGLE");

            assert!(result.is_some());
            let set = result.unwrap();
            assert!(set.contains("item1"));
            assert_eq!(set.len(), 1);

            unsafe { env::remove_var("AHMA_TEST_SINGLE") };
        }

        #[test]
        fn parses_multiple_comma_separated_items() {
            // SAFETY: Test runs in isolation, env var manipulation is safe
            unsafe { env::set_var("AHMA_TEST_MULTI", "item1,item2,item3") };

            let result = parse_env_list("AHMA_TEST_MULTI");

            assert!(result.is_some());
            let set = result.unwrap();
            assert!(set.contains("item1"));
            assert!(set.contains("item2"));
            assert!(set.contains("item3"));
            assert_eq!(set.len(), 3);

            unsafe { env::remove_var("AHMA_TEST_MULTI") };
        }

        #[test]
        fn trims_whitespace_and_lowercases() {
            // SAFETY: Test runs in isolation, env var manipulation is safe
            unsafe { env::set_var("AHMA_TEST_WHITESPACE", " Item1 , ITEM2 , item3 ") };

            let result = parse_env_list("AHMA_TEST_WHITESPACE");

            assert!(result.is_some());
            let set = result.unwrap();
            assert!(set.contains("item1"));
            assert!(set.contains("item2"));
            assert!(set.contains("item3"));

            unsafe { env::remove_var("AHMA_TEST_WHITESPACE") };
        }

        #[test]
        fn filters_out_empty_entries() {
            // SAFETY: Test runs in isolation, env var manipulation is safe
            unsafe { env::set_var("AHMA_TEST_EMPTY", "item1,,item2, ,item3") };

            let result = parse_env_list("AHMA_TEST_EMPTY");

            assert!(result.is_some());
            let set = result.unwrap();
            assert_eq!(set.len(), 3);
            assert!(set.contains("item1"));
            assert!(set.contains("item2"));
            assert!(set.contains("item3"));

            unsafe { env::remove_var("AHMA_TEST_EMPTY") };
        }
    }

    // =========================================================================
    // Tests for: should_skip
    // =========================================================================
    mod should_skip_tests {
        use super::*;

        #[test]
        fn returns_false_when_set_is_none() {
            let set: Option<HashSet<String>> = None;

            let result = should_skip(&set, "anything");

            assert!(!result);
        }

        #[test]
        fn returns_true_when_value_in_set() {
            let mut set = HashSet::new();
            set.insert("skip_me".to_string());
            let set = Some(set);

            let result = should_skip(&set, "skip_me");

            assert!(result);
        }

        #[test]
        fn returns_false_when_value_not_in_set() {
            let mut set = HashSet::new();
            set.insert("skip_me".to_string());
            let set = Some(set);

            let result = should_skip(&set, "keep_me");

            assert!(!result);
        }

        #[test]
        fn performs_case_insensitive_check() {
            let mut set = HashSet::new();
            set.insert("skip_me".to_string());
            let set = Some(set);

            // should_skip lowercases the value before checking
            assert!(should_skip(&set, "SKIP_ME"));
            assert!(should_skip(&set, "Skip_Me"));
        }

        #[test]
        fn returns_false_for_empty_set() {
            let set: Option<HashSet<String>> = Some(HashSet::new());

            let result = should_skip(&set, "anything");

            assert!(!result);
        }
    }

    // =========================================================================
    // Tests for: resolve_cli_subcommand
    // =========================================================================
    mod resolve_cli_subcommand_tests {
        use super::*;

        #[test]
        fn resolves_default_subcommand_when_tool_name_equals_config_key() {
            let config = make_tool_config_with_subcommands(
                "cargo",
                "cargo",
                true,
                vec![make_subcommand("default", true)],
            );

            let result = resolve_cli_subcommand("cargo", &config, "cargo", None);

            assert!(result.is_ok());
            let (sub, parts) = result.unwrap();
            assert_eq!(sub.name, "default");
            assert_eq!(parts, vec!["cargo"]);
        }

        #[test]
        fn resolves_explicit_subcommand() {
            let config = make_tool_config_with_subcommands(
                "cargo",
                "cargo",
                true,
                vec![
                    make_subcommand("default", true),
                    make_subcommand("build", true),
                ],
            );

            let result = resolve_cli_subcommand("cargo", &config, "cargo_build", None);

            assert!(result.is_ok());
            let (sub, parts) = result.unwrap();
            assert_eq!(sub.name, "build");
            assert_eq!(parts, vec!["cargo", "build"]);
        }

        #[test]
        fn resolves_nested_subcommand() {
            let config = make_tool_config_with_subcommands(
                "git",
                "git",
                true,
                vec![make_subcommand_with_nested(
                    "remote",
                    true,
                    vec![make_subcommand("add", true)],
                )],
            );

            let result = resolve_cli_subcommand("git", &config, "git_remote_add", None);

            assert!(result.is_ok());
            let (sub, parts) = result.unwrap();
            assert_eq!(sub.name, "add");
            assert_eq!(parts, vec!["git", "remote", "add"]);
        }

        #[test]
        fn returns_error_when_subcommand_not_found() {
            let config = make_tool_config_with_subcommands(
                "cargo",
                "cargo",
                true,
                vec![make_subcommand("build", true)],
            );

            let result = resolve_cli_subcommand("cargo", &config, "cargo_test", None);

            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("Subcommand"));
            assert!(err.contains("not found"));
        }

        #[test]
        fn returns_error_when_no_subcommands_defined() {
            let config = make_tool_config("cargo", "cargo", true);

            let result = resolve_cli_subcommand("cargo", &config, "cargo_build", None);

            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("no subcommands defined"));
        }

        #[test]
        fn ignores_disabled_subcommands() {
            let config = make_tool_config_with_subcommands(
                "cargo",
                "cargo",
                true,
                vec![
                    make_subcommand("build", false), // disabled
                    make_subcommand("test", true),
                ],
            );

            let result = resolve_cli_subcommand("cargo", &config, "cargo_build", None);

            assert!(result.is_err());
        }

        #[test]
        fn uses_subcommand_override_when_provided() {
            let config = make_tool_config_with_subcommands(
                "cargo",
                "cargo",
                true,
                vec![
                    make_subcommand("build", true),
                    make_subcommand("test", true),
                ],
            );

            let result = resolve_cli_subcommand("cargo", &config, "cargo", Some("test"));

            assert!(result.is_ok());
            let (sub, parts) = result.unwrap();
            assert_eq!(sub.name, "test");
            assert_eq!(parts, vec!["cargo", "test"]);
        }

        #[test]
        fn handles_explicit_default_subcommand_override() {
            let config = make_tool_config_with_subcommands(
                "cargo",
                "cargo",
                true,
                vec![make_subcommand("default", true)],
            );

            let result =
                resolve_cli_subcommand("cargo", &config, "cargo_something", Some("default"));

            assert!(result.is_ok());
            let (sub, _) = result.unwrap();
            assert_eq!(sub.name, "default");
        }
    }
}
