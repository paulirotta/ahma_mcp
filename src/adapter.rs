use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;

use crate::cli_parser::{CliParser, CliStructure};
use crate::config::Config;
use crate::mcp_schema::McpSchemaGenerator;
use crate::operation_monitor::{MonitorConfig, OperationMonitor};
use crate::shell_pool::{ShellCommand, ShellPoolConfig, ShellPoolManager};
use uuid::Uuid;

/// The main adapter that handles dynamic CLI tool adaptation.
#[derive(Debug)]
pub struct Adapter {
    /// Parsed CLI structures for each tool
    cli_structures: HashMap<String, CliStructure>,
    /// Configuration for each tool
    configs: HashMap<String, Config>,
    /// Schema generator for MCP tools
    schema_generator: McpSchemaGenerator,
    /// Operation monitor for async operations
    operation_monitor: Arc<OperationMonitor>,
    /// CLI parser for discovering new tools
    cli_parser: CliParser,
    /// Whether to run in synchronous mode
    synchronous_mode: bool,
    /// Command timeout in seconds
    timeout_secs: u64,
    /// Pre-warmed shell pool manager for async execution
    shell_pool: Arc<ShellPoolManager>,
}

impl Adapter {
    /// Create a new adapter instance.
    pub fn new(synchronous_mode: bool) -> Result<Self> {
        // Initialize shell pool manager and start background tasks
        let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));
        shell_pool.clone().start_background_tasks();

        Ok(Adapter {
            cli_structures: HashMap::new(),
            configs: HashMap::new(),
            schema_generator: McpSchemaGenerator::new(),
            operation_monitor: Arc::new(OperationMonitor::new(MonitorConfig::default())),
            cli_parser: CliParser::new()?,
            synchronous_mode,
            timeout_secs: 300,
            shell_pool,
        })
    }

    /// Create a new adapter with timeout
    pub fn with_timeout(synchronous_mode: bool, timeout_secs: u64) -> Result<Self> {
        let mut adapter = Self::new(synchronous_mode)?;
        adapter.timeout_secs = timeout_secs;
        Ok(adapter)
    }

    /// Execute a tool with the given arguments
    pub async fn execute_tool(&self, tool_name: &str, args: Vec<String>) -> Result<String> {
        self.execute_tool_in_dir(tool_name, args, None).await
    }

    /// Execute a tool with the given arguments in an optional working directory
    pub async fn execute_tool_in_dir(
        &self,
        tool_name: &str,
        args: Vec<String>,
        working_directory: Option<String>,
    ) -> Result<String> {
        let config = self
            .configs
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_name))?;

        // Build the command - handle Optional command properly
        let command = config.command.as_deref().unwrap_or(tool_name);
        let mut cmd_args = vec![command.to_string()];
        cmd_args.extend(args);

        // Determine sync/async based on global mode, tool-level default, and per-command override
        let mut run_sync = self.synchronous_mode || config.synchronous.unwrap_or(false);
        // If first arg after command is a subcommand, check override
        if let Some(subcmd) = cmd_args.get(1)
            && let Some(override_cfg) = config.get_command_override(subcmd)
            && let Some(sync) = override_cfg.synchronous
        {
            run_sync = sync;
        }

        if run_sync {
            self.execute_sync_in_dir(&cmd_args, working_directory.as_deref())
                .await
        } else {
            self.execute_async_in_dir(&cmd_args, working_directory.as_deref())
                .await
        }
    }

    /// Execute command synchronously
    async fn execute_sync(&self, args: &[String]) -> Result<String> {
        self.execute_sync_in_dir(args, None).await
    }

    /// Execute command synchronously with optional working directory
    async fn execute_sync_in_dir(
        &self,
        args: &[String],
        working_directory: Option<&str>,
    ) -> Result<String> {
        let mut cmd = Command::new(&args[0]);
        if args.len() > 1 {
            cmd.args(&args[1..]);
        }

        if let Some(dir) = working_directory {
            cmd.current_dir(dir);
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await
        .context("Command timed out")?
        .context("Failed to execute command")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("Command failed: {}", stderr))
        }
    }

    /// Execute command asynchronously using pre-warmed shell when possible
    async fn execute_async_in_dir(
        &self,
        args: &[String],
        working_directory: Option<&str>,
    ) -> Result<String> {
        // If we have a working directory and shell pooling is enabled, use it
        if let Some(dir) = working_directory {
            if let Some(mut shell) = self.shell_pool.get_shell(dir).await {
                let cmd = ShellCommand {
                    id: Uuid::new_v4().to_string(),
                    command: args.to_vec(),
                    working_dir: dir.to_string(),
                    timeout_ms: self.timeout_secs * 1000,
                };

                let resp = shell.execute_command(cmd).await;
                // Return shell to pool regardless of outcome
                self.shell_pool.return_shell(shell).await;

                match resp {
                    Ok(r) if r.exit_code == 0 => Ok(r.stdout),
                    Ok(r) => Err(anyhow::anyhow!(
                        "Command failed (code {}): {}",
                        r.exit_code,
                        r.stderr
                    )),
                    Err(e) => Err(anyhow::anyhow!("Shell execution error: {}", e)),
                }
            } else {
                // Fallback to sync in specified directory if pool unavailable
                self.execute_sync_in_dir(args, Some(dir)).await
            }
        } else {
            // No working directory provided; fallback to sync
            self.execute_sync(args).await
        }
    }

    /// Initialize the adapter with tools from the tools directory.
    pub async fn initialize(&mut self) -> Result<()> {
        let tools_dir = PathBuf::from("tools");
        if !tools_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(tools_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let tool_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();

                let config = Config::load_from_file(&path)?;

                // Only add the tool if it's enabled
                if config.is_enabled() {
                    self.add_tool(&tool_name, config).await?;
                }
            }
        }

        Ok(())
    }

    /// Add a tool to the adapter.
    pub async fn add_tool(&mut self, tool_name: &str, config: Config) -> Result<()> {
        // Get command name (use tool_name if command is not specified)
        let command_name = config.command.as_deref().unwrap_or(tool_name);

        // Get help output and parse CLI structure
        let help_output = self.cli_parser.get_help_output(command_name)?;
        let structure = self
            .cli_parser
            .parse_help_output(command_name, &help_output)?;

        let mut structure = structure;

        // Special handling for cargo: also parse `cargo --list` to discover installed subcommands
        if command_name == "cargo"
            && let Ok(output) = std::process::Command::new(command_name)
                .arg("--list")
                .output()
            && output.status.success()
        {
            let list_text = String::from_utf8_lossy(&output.stdout);
            let mut names = std::collections::HashSet::new();
            for sc in &structure.subcommands {
                names.insert(sc.name.clone());
            }
            for line in list_text.lines() {
                // cargo --list often prints entries like:
                //    build                Compile the current package
                //    test                 Run the tests
                if let Ok(Some(mut sc)) = self.cli_parser.parse_subcommand_line(line)
                    && !names.contains(&sc.name)
                {
                    sc.description = sc.description.trim().to_string();
                    structure.subcommands.push(sc.clone());
                    names.insert(sc.name);
                }
            }
        }

        self.cli_structures.insert(tool_name.to_string(), structure);
        self.configs.insert(tool_name.to_string(), config);

        Ok(())
    }

    /// Get all available tool schemas for MCP.
    pub fn get_tool_schemas(&self) -> Result<Vec<Value>> {
        let mut schemas = Vec::new();

        for (tool_name, structure) in &self.cli_structures {
            let config = self.configs.get(tool_name).unwrap(); // Safe since we control insertion
            let schema = self
                .schema_generator
                .generate_tool_schema(structure, config)?;
            schemas.push(schema);
        }

        Ok(schemas)
    }

    /// Get the status of an operation.
    pub async fn get_operation_status(&self, operation_id: &str) -> Result<Value> {
        if let Some(operation) = self.operation_monitor.get_operation(operation_id).await {
            // Extract output and error from the result field
            let (output, error) = match &operation.result {
                Some(Ok(output_str)) => (Some(output_str.clone()), None),
                Some(Err(error_str)) => (None, Some(error_str.clone())),
                None => (None, None),
            };

            Ok(serde_json::json!({
                "id": operation.id,
                "state": format!("{:?}", operation.state),
                "output": output,
                "error": error
            }))
        } else {
            Err(anyhow::anyhow!("Operation not found: {}", operation_id))
        }
    }

    /// Wait for multiple operations to complete (simplified implementation).
    pub async fn wait_for_operations(&self, operation_ids: &[String]) -> Result<Vec<Value>> {
        let mut results = Vec::new();

        for operation_id in operation_ids {
            let result = self.get_operation_status(operation_id).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get tool hint for a command.
    pub fn get_tool_hint(&self, _tool_name: &str, operation_type: &str) -> String {
        // Simple implementation using the available API
        crate::tool_hints::preview("operation", operation_type)
    }
}
// TODO: Add performance benchmarks for tool discovery
