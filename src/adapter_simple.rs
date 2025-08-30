use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;

use crate::cli_parser::{CliParser, CliStructure};
use crate::config::Config;
use crate::mcp_schema::McpSchemaGenerator;
use crate::operation_monitor::{MonitorConfig, OperationMonitor};
use crate::shell_pool::{ShellPoolConfig, ShellPoolManager};

/// The main adapter that handles dynamic CLI tool adaptation.
pub struct Adapter {
    /// Parsed CLI structures for each tool
    cli_structures: HashMap<String, CliStructure>,
    /// Configuration for each tool
    configs: HashMap<String, Config>,
    /// Schema generator for MCP tools
    schema_generator: McpSchemaGenerator,
    /// Operation monitor for async operations
    operation_monitor: OperationMonitor,
    /// Shell pool for command execution  
    shell_pool: ShellPoolManager,
    /// CLI parser for discovering new tools
    cli_parser: CliParser,
    /// Whether to run in synchronous mode
    synchronous_mode: bool,
}

impl Adapter {
    /// Create a new adapter instance.
    pub fn new(synchronous_mode: bool) -> Result<Self> {
        Ok(Adapter {
            cli_structures: HashMap::new(),
            configs: HashMap::new(),
            schema_generator: McpSchemaGenerator::new(),
            operation_monitor: OperationMonitor::new(MonitorConfig::default()),
            shell_pool: ShellPoolManager::new(ShellPoolConfig::default()),
            cli_parser: CliParser::new()?,
            synchronous_mode,
        })
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
                self.add_tool(&tool_name, config).await?;
            }
        }

        Ok(())
    }

    /// Add a tool to the adapter.
    pub async fn add_tool(&mut self, tool_name: &str, config: Config) -> Result<()> {
        // Get help output and parse CLI structure
        let help_output = self.cli_parser.get_help_output(&config.command)?;
        let structure = self
            .cli_parser
            .parse_help_output(&config.command, &help_output)?;

        self.cli_structures.insert(tool_name.to_string(), structure);
        self.configs.insert(tool_name.to_string(), config);

        Ok(())
    }

    /// Get all available tool schemas for MCP.
    pub fn get_tool_schemas(&self) -> Result<Vec<Value>> {
        let mut schemas = Vec::new();

        for (tool_name, structure) in &self.cli_structures {
            let config = self.configs.get(tool_name);
            let schema = self
                .schema_generator
                .generate_tool_schema(structure, config)?;
            schemas.push(schema);
        }

        Ok(schemas)
    }

    /// Execute a tool command.
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: &HashMap<String, Value>,
    ) -> Result<Value> {
        let _structure = self
            .cli_structures
            .get(tool_name)
            .with_context(|| format!("Unknown tool: {}", tool_name))?;

        let config = self
            .configs
            .get(tool_name)
            .with_context(|| format!("No configuration for tool: {}", tool_name))?;

        // Build command arguments from parameters
        let mut command_args = vec![config.command.clone()];

        // Add subcommand if specified
        if let Some(subcommand) = parameters.get("subcommand") {
            if let Some(subcommand_str) = subcommand.as_str() {
                command_args.push(subcommand_str.to_string());
            }
        }

        // Add other parameters as flags
        for (key, value) in parameters {
            if key == "subcommand" {
                continue;
            }

            if let Some(bool_val) = value.as_bool() {
                if bool_val {
                    command_args.push(format!("--{}", key));
                }
            } else if let Some(str_val) = value.as_str() {
                command_args.push(format!("--{}", key));
                command_args.push(str_val.to_string());
            }
        }

        let working_directory = config
            .working_directory
            .clone()
            .unwrap_or_else(|| ".".to_string());

        if self.synchronous_mode {
            self.execute_synchronous(&command_args, &working_directory)
                .await
        } else {
            let operation_id = self
                .execute_asynchronous(&command_args, &working_directory, tool_name)
                .await?;
            Ok(serde_json::json!({
                "operation_id": operation_id,
                "status": "started"
            }))
        }
    }

    /// Execute command synchronously.
    async fn execute_synchronous(
        &self,
        command_args: &[String],
        working_directory: &str,
    ) -> Result<Value> {
        let mut command = Command::new(&command_args[0]);
        if command_args.len() > 1 {
            command.args(&command_args[1..]);
        }
        command.current_dir(working_directory);

        let output = command.output().await?;

        Ok(serde_json::json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code()
        }))
    }

    /// Execute command asynchronously.
    async fn execute_asynchronous(
        &self,
        command_args: &[String],
        working_directory: &str,
        tool_name: &str,
    ) -> Result<String> {
        let command_str = command_args.join(" ");
        let description = format!("Running {} command", tool_name);

        let operation_id = self
            .operation_monitor
            .register_operation(
                command_str,
                description,
                Some(std::time::Duration::from_secs(300)), // 5 minute timeout
                Some(working_directory.to_string()),
            )
            .await;

        // Start the operation
        self.operation_monitor
            .start_operation(&operation_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start operation: {}", e))?;

        // Execute the command in a background task
        let op_id = operation_id.clone();
        let command_args_owned = command_args.to_vec();
        let working_dir = working_directory.to_string();

        tokio::spawn(async move {
            let mut command = Command::new(&command_args_owned[0]);
            if command_args_owned.len() > 1 {
                command.args(&command_args_owned[1..]);
            }
            command.current_dir(&working_dir);

            match command.output().await {
                Ok(output) => {
                    let result = serde_json::json!({
                        "stdout": String::from_utf8_lossy(&output.stdout),
                        "stderr": String::from_utf8_lossy(&output.stderr),
                        "exit_code": output.status.code()
                    });

                    // Note: We can't easily complete the operation here since we don't have
                    // access to the monitor. In a real implementation, we'd need to pass
                    // a callback or use a different architecture.
                    tracing::info!("Command completed: {}", result);
                }
                Err(e) => {
                    tracing::error!("Command failed: {}", e);
                }
            }
        });

        Ok(operation_id)
    }

    /// Get the status of an operation.
    pub async fn get_operation_status(&self, operation_id: &str) -> Result<Value> {
        if let Some(operation) = self.operation_monitor.get_operation(operation_id).await {
            Ok(serde_json::json!({
                "id": operation.id,
                "state": format!("{:?}", operation.state),
                "output": operation.output,
                "error": operation.error_message
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
