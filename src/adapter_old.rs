use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;

use crate::cli_parser::{CliParser, CliStructure};
use crate::config::Config;
use crate::mcp_schema::McpSchemaGenerator;
use crate::operation_monitor::{OperationMonitor, MonitorConfig};
use crate::shell_pool::{ShellPoolManager, ShellPoolConfig};
use crate::tool_hints;

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
            callback_system: CallbackSystem::new(),
            cli_parser: CliParser::new()?,
            synchronous_mode,
        })
    }
    
    /// Initialize the adapter with tools from the tools directory.
    pub async fn initialize(&mut self) -> Result<()> {
        // Scan tools directory for configuration files
        let tools_dir = std::path::Path::new("tools");
        if !tools_dir.exists() {
            std::fs::create_dir_all(tools_dir)
                .context("Failed to create tools directory")?;
        }
        
        // Load all tool configurations
        if let Ok(entries) = std::fs::read_dir(tools_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension() == Some(std::ffi::OsStr::new("toml")) {
                    if let Some(stem) = path.file_stem() {
                        if let Some(tool_name) = stem.to_str() {
                            if let Ok(config) = Config::load_from_file(&path) {
                                // Only load enabled tools
                                if config.is_enabled() {
                                    // Parse CLI structure for this tool
                                    match self.cli_parser.parse_tool(config.get_command()) {
                                        Ok(structure) => {
                                            self.cli_structures.insert(tool_name.to_string(), structure);
                                            self.configs.insert(tool_name.to_string(), config);
                                        }
                                        Err(e) => {
                                            tracing::warn!("Failed to parse tool '{}': {}", tool_name, e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        tracing::info!("Initialized adapter with {} tools", self.cli_structures.len());
        Ok(())
    }
    
    /// Get the list of available MCP tools.
    pub fn get_tools(&self) -> Result<Value> {
        let tools: Vec<_> = self.cli_structures.iter()
            .filter_map(|(name, structure)| {
                if let Some(config) = self.configs.get(name) {
                    self.schema_generator.generate_tool_schema(structure, config).ok()
                } else {
                    None
                }
            })
            .collect();
            
        Ok(serde_json::json!({
            "tools": tools
        }))
    }
    
    /// Execute a tool command based on MCP parameters.
    pub async fn execute_tool(&mut self, tool_name: &str, params: &Value) -> Result<Value> {
        let structure = self.cli_structures.get(tool_name)
            .context(format!("Unknown tool: {}", tool_name))?;
        let config = self.configs.get(tool_name)
            .context(format!("No config for tool: {}", tool_name))?;
            
        // Parse parameters
        let working_directory = params.get("working_directory")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
            
        let enable_async = params.get("enable_async_notification")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
            
        let operation_id = params.get("operation_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("op_{}_{}", tool_name, Uuid::new_v4().simple()));
            
        // Build the command
        let mut command_args = vec![config.get_command().to_string()];
        
        // Add subcommand if specified
        if let Some(subcommand) = params.get("subcommand").and_then(|v| v.as_str()) {
            command_args.push(subcommand.to_string());
        }
        
        // Add parsed options
        if let Some(structure) = self.cli_structures.get(tool_name) {
            for option in &structure.global_options {
                if let Some(param_name) = self.get_option_param_name(option) {
                    if let Some(value) = params.get(&param_name) {
                        self.add_option_to_command(&mut command_args, option, value)?;
                    }
                }
            }
        }
        
        // Add raw args if provided
        if let Some(args) = params.get("args").and_then(|v| v.as_array()) {
            for arg in args {
                if let Some(arg_str) = arg.as_str() {
                    command_args.push(arg_str.to_string());
                }
            }
        }
        
        // Execute the command
        if self.synchronous_mode || !enable_async {
            self.execute_synchronous(&command_args, working_directory, config).await
        } else {
            self.execute_asynchronous(&command_args, working_directory, config, &operation_id, tool_name).await
        }
    }
    
    /// Execute a command synchronously and return the result immediately.
    async fn execute_synchronous(&self, args: &[String], working_dir: &str, config: &Config) -> Result<Value> {
        let mut command = Command::new(&args[0]);
        if args.len() > 1 {
            command.args(&args[1..]);
        }
        command.current_dir(working_dir);
        
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(config.get_timeout_seconds()),
            command.output()
        )
        .await
        .context("Command timed out")?
        .context("Failed to execute command")?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        Ok(serde_json::json!({
            "success": output.status.success(),
            "exit_code": output.status.code(),
            "stdout": stdout,
            "stderr": stderr
        }))
    }
    
    /// Execute a command asynchronously and return operation ID.
    async fn execute_asynchronous(
        &mut self, 
        args: &[String], 
        working_dir: &str, 
        config: &Config,
        operation_id: &str,
        tool_name: &str
    ) -> Result<Value> {
        // Register the operation
        self.operation_monitor.add_operation(
            operation_id.to_string(),
            args.join(" "),
            working_dir.to_string()
        ).await;
        
        // Get appropriate hint for the operation
        let subcommand = if args.len() > 1 { Some(args[1].as_str()) } else { None };
        let hint = config.get_hint(subcommand)
            .or_else(|| get_tool_hint(tool_name, subcommand))
            .unwrap_or("Operation running in background");
            
        // Start the command in background
        let op_id = operation_id.to_string();
        let args_clone = args.to_vec();
        let working_dir = working_dir.to_string();
        let timeout = config.get_timeout_seconds();
        let mut monitor = self.operation_monitor.clone();
        let mut shell_pool = self.shell_pool.clone();
        
        tokio::spawn(async move {
            let result = Self::run_command_in_background(
                &args_clone,
                &working_dir,
                timeout,
                &op_id,
                &mut monitor,
                &mut shell_pool
            ).await;
            
            match result {
                Ok(output) => {
                    monitor.update_operation(&op_id, OperationUpdate {
                        state: OperationState::Completed,
                        output: Some(output),
                        error: None,
                    }).await.ok();
                }
                Err(e) => {
                    monitor.update_operation(&op_id, OperationUpdate {
                        state: OperationState::Failed,
                        output: None,
                        error: Some(e.to_string()),
                    }).await.ok();
                }
            }
        });
        
        // Return immediate response with operation info
        Ok(serde_json::json!({
            "operation_id": operation_id,
            "status": "started",
            "hint": hint,
            "message": format!("Operation {} started in background", operation_id)
        }))
    }
    
    /// Run a command in the background using the shell pool.
    async fn run_command_in_background(
        args: &[String],
        working_dir: &str,
        timeout_seconds: u64,
        operation_id: &str,
        monitor: &mut OperationMonitor,
        shell_pool: &mut ShellPoolManager
    ) -> Result<String> {
        // Update operation state to running
        monitor.update_operation(operation_id, OperationUpdate {
            state: OperationState::Running,
            output: None,
            error: None,
        }).await?;
        
        // Execute command using shell pool
        let shell_command = args.join(" ");
        let result = shell_pool.execute_command(&shell_command, working_dir, timeout_seconds).await?;
        
        Ok(result)
    }
    
    /// Convert CLI option to command arguments.
    fn add_option_to_command(&self, args: &mut Vec<String>, option: &crate::cli_parser::CliOption, value: &Value) -> Result<()> {
        if option.takes_value {
            // Add option with value
            if let Some(long) = &option.long {
                args.push(format!("--{}", long));
                if let Some(val_str) = value.as_str() {
                    args.push(val_str.to_string());
                }
            } else if let Some(short) = option.short {
                args.push(format!("-{}", short));
                if let Some(val_str) = value.as_str() {
                    args.push(val_str.to_string());
                }
            }
        } else {
            // Boolean flag
            if let Some(true) = value.as_bool() {
                if let Some(long) = &option.long {
                    args.push(format!("--{}", long));
                } else if let Some(short) = option.short {
                    args.push(format!("-{}", short));
                }
            }
        }
        
        Ok(())
    }
    
    /// Get parameter name for a CLI option.
    fn get_option_param_name(&self, option: &crate::cli_parser::CliOption) -> Option<String> {
        option.long.clone().or_else(|| option.short.map(|c| c.to_string()))
    }
    
    /// Get the status of an operation.
    pub async fn get_operation_status(&self, operation_id: &str) -> Result<Value> {
        let status = self.operation_monitor.get_operation_status(operation_id).await?;
        Ok(serde_json::json!({
            "operation_id": operation_id,
            "status": status
        }))
    }
    
    /// Wait for operations to complete.
    pub async fn wait_for_operations(&self, operation_ids: &[String]) -> Result<Value> {
        let results = self.operation_monitor.wait_for_operations(operation_ids).await?;
        Ok(serde_json::json!({
            "results": results
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::write;

    #[tokio::test]
    async fn test_adapter_initialization() {
        let temp_dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();
        
        // Create tools directory with a test config
        std::fs::create_dir_all("tools").unwrap();
        let config_content = r#"
tool_name = "echo"
enabled = true
timeout_seconds = 60
"#;
        write("tools/echo.toml", config_content).unwrap();
        
        let mut adapter = Adapter::new(false).unwrap();
        let result = adapter.initialize().await;
        
        std::env::set_current_dir(original_dir).unwrap();
        
        assert!(result.is_ok());
        assert!(adapter.cli_structures.contains_key("echo"));
        assert!(adapter.configs.contains_key("echo"));
    }
    
    #[tokio::test] 
    async fn test_get_tools() {
        let mut adapter = Adapter::new(false).unwrap();
        
        // Add a mock tool manually
        let structure = crate::cli_parser::CliStructure::new("test".to_string());
        let config = Config::default();
        
        adapter.cli_structures.insert("test".to_string(), structure);
        adapter.configs.insert("test".to_string(), config);
        
        let tools = adapter.get_tools().unwrap();
        assert!(tools["tools"].is_array());
        assert!(tools["tools"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_synchronous_execution() {
        let adapter = Adapter::new(true).unwrap();
        let config = Config::default();
        
        // Test with echo command
        let args = vec!["echo".to_string(), "hello world".to_string()];
        let result = adapter.execute_synchronous(&args, ".", &config).await;
        
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output["success"].as_bool().unwrap());
        assert!(output["stdout"].as_str().unwrap().contains("hello world"));
    }
}
