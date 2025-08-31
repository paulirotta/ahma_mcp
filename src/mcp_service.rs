//! # Ahma MCP Service Implementation
//!
//! This module contains the core implementation of the `ahma_mcp` server. The
//! `AhmaMcpService` struct implements the `rmcp::ServerHandler` trait, making it the
//! central point for handling all incoming MCP requests from a client.
//!
//! ## Core Components
//!
//! - **`AhmaMcpService`**: The main struct that holds the application's state, including
//!   the `Adapter` for tool execution and a map of all loaded tool configurations
//!   (`tools_config`).
//!
//! ## Key `ServerHandler` Trait Implementations
//!
//! - **`get_info()`**: Provides the client with initial information about the server,
//!   including its name, version, and, most importantly, a set of `instructions`. These
//!   instructions are crucial for guiding the AI agent on how to interact with this
//!   specific server, explaining the tool naming convention (`<tool>_<subcommand>`),
//!   the requirement for a `working_directory`, and how to use asynchronous execution.
//!
//! - **`list_tools()`**: This is the heart of the dynamic tool discovery mechanism. It
//!   iterates through the `tools_config` map and generates a `Tool` definition for each
//!   subcommand of each configured CLI tool.
//!   - It constructs a unique `name` for each tool-subcommand pair (e.g., `cargo_build`).
//!   - It generates a descriptive `description`, often enriching it with usage hints from
//!     the configuration and special tips for potentially long-running commands.
//!   - It calls `cli_options_to_schema` to generate the `input_schema` for the tool,
//!     which defines the parameters the client can pass.
//!
//! - **`call_tool()`**: This method handles the execution of a tool.
//!   - It parses the tool name (e.g., `cargo_build`) to identify the base tool (`cargo`)
//!     and the command to run (`build`).
//!   - It meticulously translates the incoming JSON `arguments` into a vector of
//!     command-line arguments, handling boolean flags and valued options.
//!   - It extracts the mandatory `working_directory` and any raw `args`.
//!   - It then delegates the actual execution to `self.adapter.execute_tool_in_dir()`.
//!   - Finally, it wraps the `Ok` result in a `CallToolResult::success` or maps the `Err`
//!     to an appropriate `McpError`.
//!
//! ## Schema Generation (`cli_options_to_schema`)
//!
//! This helper function is responsible for creating the `input_schema` for a given tool.
//! It converts a slice of `CliOption`s into a JSON schema `object`, defining the `type`
//! and `description` for each parameter. Crucially, it also injects the common parameters
//! required by the `ahma_mcp` server, such as `working_directory`, `args`, and the
//! async-related flags, ensuring that every tool exposed by the server has a consistent
//! interface for these core features.
//!
//! ## Server Startup
//!
//! The `start_server()` method provides a convenient way to launch the service, wiring it
//! up to a standard I/O transport (`stdio`) and running it until completion.

use crate::{
    adapter::{Adapter, ExecutionMode},
    config::{CliOption, Config},
};
use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, model::*,
    service::RequestContext, transport::stdio,
};
use serde_json::{Map, Value, json};
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// JSON object type alias to match rmcp expectations
type JsonObject = Map<String, Value>;

/// Core MCP service that dynamically exposes CLI tools as MCP tools
#[derive(Debug)]
pub struct AhmaMcpService {
    adapter: Arc<Adapter>,
    /// Map of tool base names (e.g., "cargo") to their full configuration.
    tools_config: Arc<RwLock<HashMap<String, Config>>>,
}

impl AhmaMcpService {
    /// Create a new MCP service with the given adapter and initial tools.
    pub async fn new(adapter: Arc<Adapter>, configs: Vec<(String, Config)>) -> Result<Self> {
        let mut tools_config = HashMap::new();
        for (name, config) in configs {
            tools_config.insert(name, config);
        }

        Ok(Self {
            adapter,
            tools_config: Arc::new(RwLock::new(tools_config)),
        })
    }

    /// Convert our `CliOption` model to a JSON schema property.
    fn cli_option_to_json_schema(option: &CliOption) -> (String, Value) {
        let mut property = Map::new();
        let json_type = match option.type_.as_str() {
            "boolean" => "boolean",
            "integer" => "integer",
            "array" => "array",
            _ => "string", // Default to string for "string" and any other value
        };
        property.insert("type".to_string(), json!(json_type));
        property.insert("description".to_string(), json!(&option.description));

        if json_type == "array" {
            property.insert("items".to_string(), json!({"type": "string"}));
        }

        (option.name.clone(), json!(property))
    }

    /// Generate the MCP input schema for a given set of `CliOption`s.
    fn build_input_schema(options: &[CliOption]) -> Arc<JsonObject> {
        let mut properties = Map::new();
        let mut required = Vec::new();

        // Add tool-specific options
        for option in options {
            let (name, schema) = Self::cli_option_to_json_schema(option);
            properties.insert(name, schema);
        }

        // Add common ahma_mcp parameters
        properties.insert(
            "working_directory".to_string(),
            json!({
                "type": "string",
                "description": "Absolute path to the directory where the command will run (required)."
            }),
        );
        required.push("working_directory".to_string());

        properties.insert(
            "args".to_string(),
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": "Additional raw arguments to pass to the command (e.g., for positional args or complex flags)."
            }),
        );

        properties.insert(
            "enable_async_notification".to_string(),
            json!({
                "type": "boolean",
                "description": "Set to true for non-blocking execution of long-running commands (e.g., build, test).",
                "default": false
            }),
        );

        properties.insert(
            "operation_id".to_string(),
            json!({
                "type": "string",
                "description": "Optional custom ID for an asynchronous operation."
            }),
        );

        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("object"));
        schema.insert("properties".to_string(), json!(properties));
        if !required.is_empty() {
            schema.insert("required".to_string(), json!(required));
        }

        Arc::new(schema)
    }

    /// A unified argument parser to convert MCP JSON arguments into a command-line vector.
    fn parse_arguments(
        subcommand: &str,
        arguments: Value,
    ) -> Result<(Vec<String>, Option<String>), McpError> {
        let mut cmd_args = vec![subcommand.to_string()];
        let working_dir: Option<String>;

        if let Value::Object(mut args_map) = arguments {
            // Extract working_directory first
            working_dir = args_map
                .remove("working_directory")
                .and_then(|v| v.as_str().map(String::from));

            // Extract raw `args` next
            let mut raw_args = Vec::new();
            if let Some(Value::Array(args_vec)) = args_map.remove("args") {
                for v in args_vec {
                    if let Some(s) = v.as_str() {
                        raw_args.push(s.to_string());
                    }
                }
            }

            // Process remaining keys as flags
            for (key, value) in args_map {
                // Skip async control flags
                if key == "enable_async_notification" || key == "operation_id" {
                    continue;
                }

                let flag = if key.len() == 1 {
                    format!("-{}", key)
                } else {
                    format!("--{}", key)
                };

                match value {
                    Value::Bool(true) => cmd_args.push(flag),
                    Value::String(s) => {
                        cmd_args.push(flag);
                        cmd_args.push(s);
                    }
                    Value::Number(n) => {
                        cmd_args.push(flag);
                        cmd_args.push(n.to_string());
                    }
                    Value::Array(arr) => {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                cmd_args.push(flag.clone());
                                cmd_args.push(s.to_string());
                            }
                        }
                    }
                    // Ignore false booleans, nulls, and objects
                    _ => {
                        debug!("Skipping argument '{:?}': unsupported type or value", key);
                    }
                }
            }

            // Append raw args at the end
            cmd_args.extend(raw_args);
        } else {
            return Err(McpError::invalid_params(
                "invalid_arguments_object",
                Some(json!({
                    "message": "Tool arguments must be a JSON object.",
                })),
            ));
        }

        Ok((cmd_args, working_dir))
    }
}

impl ServerHandler for AhmaMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "ahma_mcp".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some(
                "Ahma MCP: A generic, config-driven CLI tool adapter.\n\n\
                - Tools are exposed as `<tool>_<subcommand>` (e.g., `cargo_build`).\n\
                - All tools require a `working_directory` argument (absolute path).\n\
                - For long-running commands (build, test), set `enable_async_notification: true` to receive progress updates.\n\n\
                Example: Call `cargo_build` with arguments `{\"working_directory\": \"/path/to/project\", \"release\": true}`."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        info!("MCP list_tools request received");
        let configs = self.tools_config.read().await;
        info!(
            "Loaded {} tool configurations: {:?}",
            configs.len(),
            configs.keys().collect::<Vec<_>>()
        );

        let mut mcp_tools = Vec::new();

        for (tool_name, config) in configs.iter() {
            info!(
                "Processing tool '{}' with {} subcommands",
                tool_name,
                config.subcommand.len()
            );

            for subcommand in &config.subcommand {
                let tool_id = format!("{}_{}", tool_name, subcommand.name);
                let mut description = subcommand.description.clone();

                // Add a hint for async usage on common long-running commands
                let name_l = subcommand.name.to_lowercase();
                if ["build", "test", "bench", "run", "doc", "clippy", "nextest"]
                    .iter()
                    .any(|k| name_l.contains(k))
                {
                    description.push_str(
                        " | Tip: set 'enable_async_notification': true for non-blocking execution.",
                    );
                }

                let input_schema = Self::build_input_schema(&subcommand.options);
                info!(
                    "Generated tool '{}' with {} options",
                    tool_id,
                    subcommand.options.len()
                );
                debug!("Tool '{}' description: {}", tool_id, description);
                debug!(
                    "Tool '{}' schema properties: {}",
                    tool_id,
                    input_schema.keys().collect::<Vec<_>>().len()
                );

                mcp_tools.push(Tool {
                    name: Cow::Owned(tool_id),
                    description: Some(Cow::Owned(description)),
                    input_schema,
                    annotations: None,
                    output_schema: None,
                });
            }
        }

        info!("Returning {} MCP tools to client", mcp_tools.len());
        for tool in &mcp_tools {
            debug!("Exposing tool: {}", tool.name);
        }

        Ok(ListToolsResult {
            tools: mcp_tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        CallToolRequestParam { name, arguments }: CallToolRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Executing tool: {} with args: {:?}", name, arguments);

        let parts: Vec<&str> = name.split('_').collect();
        if parts.len() < 2 {
            return Err(McpError::invalid_params(
                "invalid_tool_name",
                Some(json!({"tool_name": name, "expected_format": "tool_subcommand"})),
            ));
        }

        let base_tool = parts[0];
        let subcommand_name = parts[1..].join("_");

        // Find the config for the base tool and the specific subcommand
        let configs = self.tools_config.read().await;
        let config = configs.get(base_tool).ok_or_else(|| {
            McpError::invalid_params("unknown_tool", Some(json!({"tool": base_tool})))
        })?;

        let subcommand_config = config
            .subcommand
            .iter()
            .find(|sc| sc.name == subcommand_name)
            .ok_or_else(|| {
                McpError::invalid_params(
                    "unknown_subcommand",
                    Some(json!({"tool": base_tool, "subcommand": subcommand_name})),
                )
            })?;

        let (cmd_args, working_dir) = Self::parse_arguments(
            &subcommand_name,
            Value::Object(arguments.unwrap_or_default()),
        )?;

        let working_dir = working_dir.ok_or_else(|| {
            McpError::invalid_params(
                "missing_working_directory",
                Some(json!({
                    "message": "The 'working_directory' parameter is required for all tool calls.",
                })),
            )
        })?;

        // Determine execution mode: Subcommand config overrides global config.
        let exec_mode = if subcommand_config.synchronous.unwrap_or(false) {
            ExecutionMode::Synchronous
        } else {
            ExecutionMode::Asynchronous
        };

        debug!(
            "Executing command: {} with args: {:?} in directory: {} (mode: {:?})",
            &config.command, cmd_args, working_dir, exec_mode
        );

        // Clone values for error reporting before they're potentially moved
        let cmd_args_clone = cmd_args.clone();
        let working_dir_clone = working_dir.clone();

        match self
            .adapter
            .execute_tool_in_dir(
                &config.command,
                cmd_args,
                Some(working_dir),
                exec_mode,
                config.hints.clone(),
            )
            .await
        {
            Ok(result) => {
                info!("Tool '{}' executed successfully", name);
                Ok(result)
            }
            Err(e) => {
                // Provide detailed error information instead of generic failure
                error!("Tool execution failed for '{}': {}", name, e);

                // Check for common error types and provide specific guidance
                let error_message = if e.to_string().contains("No such file or directory") {
                    format!(
                        "Command '{}' not found. Please ensure it is installed and available in PATH.",
                        &config.command
                    )
                } else if e.to_string().contains("Permission denied") {
                    format!(
                        "Permission denied executing '{}'. Check file permissions and execute permissions.",
                        &config.command
                    )
                } else if e.to_string().contains("Command failed:") {
                    // This is likely stderr from the command itself - pass it through
                    format!("Tool '{}' execution failed: {}", name, e)
                } else if e.to_string().contains("timed out") {
                    format!(
                        "Tool '{}' execution timed out. Consider increasing timeout or using async execution.",
                        name
                    )
                } else {
                    // Generic error with full context
                    format!("Tool '{}' execution error: {}", name, e)
                };

                // Create a structured error with additional context for debugging
                Err(McpError::internal_error(
                    "tool_execution_failed",
                    Some(json!({
                        "tool_name": name,
                        "base_command": &config.command,
                        "arguments": cmd_args_clone,
                        "working_directory": working_dir_clone,
                        "error_message": error_message,
                        "original_error": e.to_string()
                    })),
                ))
            }
        }
    }
}

impl AhmaMcpService {
    /// Start the MCP server with stdio transport
    pub async fn start_server(self) -> Result<()> {
        info!("Starting ahma_mcp MCP server");
        let service = self.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }
}
