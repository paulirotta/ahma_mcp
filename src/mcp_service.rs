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
    adapter::Adapter,
    cli_parser::{CliOption, CliStructure},
    config::Config,
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
    tools_config: Arc<RwLock<HashMap<String, (Config, CliStructure)>>>,
}

impl AhmaMcpService {
    /// Create a new MCP service with the given adapter and initial tools
    pub async fn new(
        adapter: Arc<Adapter>,
        tools: Vec<(String, Config, CliStructure)>,
    ) -> Result<Self> {
        let mut tools_config = HashMap::new();
        for (name, config, cli_structure) in tools {
            tools_config.insert(name, (config, cli_structure));
        }

        Ok(Self {
            adapter,
            tools_config: Arc::new(RwLock::new(tools_config)),
        })
    }

    /// Add or update a tool configuration
    pub async fn update_tool(&self, name: String, config: Config, cli_structure: CliStructure) {
        let mut tools = self.tools_config.write().await;
        tools.insert(name, (config, cli_structure));
    }

    /// Remove a tool
    pub async fn remove_tool(&self, name: &str) {
        let mut tools = self.tools_config.write().await;
        tools.remove(name);
    }

    /// List all available tools
    pub async fn list_tools(&self) -> Vec<String> {
        let tools = self.tools_config.read().await;
        tools.keys().cloned().collect()
    }

    /// Convert CLI options to MCP tool schema
    fn cli_options_to_schema(options: &[CliOption]) -> Arc<JsonObject> {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for option in options {
            let mut property = Map::new();

            // Set type based on option characteristics
            if option.takes_value {
                property.insert("type".to_string(), json!("string"));
            } else {
                property.insert("type".to_string(), json!("boolean"));
            }

            // Add description
            property.insert("description".to_string(), json!(&option.description));

            // Use the long form if available, otherwise use short form
            let property_name = if let Some(long) = &option.long {
                long.clone()
            } else if let Some(short) = option.short {
                short.to_string()
            } else {
                continue; // Skip options without names
            };

            // If it's required (heuristic: no default value and described as required)
            if option.description.contains("required") || option.description.contains("mandatory") {
                required.push(property_name.clone());
            }

            properties.insert(property_name, json!(property));
        }

        // Always include working_directory and args for execution
        properties.insert(
            "working_directory".to_string(),
            json!({
                "type": "string",
                "description": "Absolute path to the directory to run the command in (required)"
            }),
        );
        required.push("working_directory".to_string());

        properties.insert(
            "args".to_string(),
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": "Additional raw arguments to pass after flags (advanced). Example: [\"-R\", \"-a\", \"-1\"] for ls recursive"
            }),
        );

        // Async-related optional fields to encourage non-blocking usage when supported by the adapter
        properties.insert(
            "enable_async_notification".to_string(),
            json!({
                "type": "boolean",
                "description": "Prefer async execution with progress notifications for long-running commands (build/test/bench)",
                "default": false
            }),
        );

        properties.insert(
            "operation_id".to_string(),
            json!({
                "type": "string",
                "description": "Optional custom operation ID when async notifications are enabled"
            }),
        );

        // Create the schema object
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("object"));
        schema.insert("properties".to_string(), json!(properties));

        if !required.is_empty() {
            schema.insert("required".to_string(), json!(required));
        }

        Arc::new(schema)
    }
}

impl AhmaMcpService {
    /// Dynamic tool execution - this will be called by MCP for any tool
    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: Value,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Executing tool: {} with args: {:?}", tool_name, arguments);

        // Parse tool name and command
        let parts: Vec<&str> = tool_name.split('_').collect();
        if parts.len() < 2 {
            return Err(McpError::invalid_params(
                "invalid_tool_name",
                Some(json!({"tool_name": tool_name, "expected_format": "toolname_command"})),
            ));
        }

        let base_tool = parts[0];
        let command = parts[1..].join("_");

        // Look up the tool configuration
        let tools = self.tools_config.read().await;
        let (_config, _cli_structure) = tools.get(base_tool).ok_or_else(|| {
            McpError::invalid_params("unknown_tool", Some(json!({"tool": base_tool})))
        })?;

        // Convert arguments to command-line args and extract working directory
        let mut cmd_args = vec![command.clone()];
        let mut working_dir: Option<String> = None;

        if let Value::Object(args_map) = arguments {
            // Pull out working_directory and args first, then convert the rest into flags
            if let Some(Value::String(dir)) = args_map.get("working_directory") {
                working_dir = Some(dir.clone());
            }
            if let Some(Value::Array(raw_args)) = args_map.get("args") {
                for v in raw_args {
                    if let Some(s) = v.as_str() {
                        cmd_args.push(s.to_string());
                    }
                }
            }

            for (key, value) in args_map {
                if key == "working_directory" || key == "args" {
                    continue;
                }
                match value {
                    Value::Bool(true) => {
                        if key.len() == 1 {
                            cmd_args.push(format!("-{}", key));
                        } else {
                            cmd_args.push(format!("--{}", key));
                        }
                    }
                    Value::String(s) => {
                        if key.len() == 1 {
                            cmd_args.push(format!("-{}", key));
                        } else {
                            cmd_args.push(format!("--{}", key));
                        }
                        cmd_args.push(s);
                    }
                    Value::Number(n) => {
                        if key.len() == 1 {
                            cmd_args.push(format!("-{}", key));
                        } else {
                            cmd_args.push(format!("--{}", key));
                        }
                        cmd_args.push(n.to_string());
                    }
                    _ => {
                        debug!("Skipping argument {}: {:?} (unsupported type)", key, value);
                    }
                }
            }
        }

        // Require working_directory for safe async behavior and isolation
        if working_dir.is_none() {
            return Err(McpError::invalid_params(
                "missing_working_directory",
                Some(json!({
                    "message": "All tools require 'working_directory' for execution",
                    "hint": "Pass input: { working_directory: <abs path> }"
                })),
            ));
        }

        // Execute the command using the adapter
        match self
            .adapter
            .execute_tool_in_dir(base_tool, cmd_args, working_dir)
            .await
        {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => {
                error!("Tool execution failed: {}", e);
                Err(McpError::internal_error(
                    "execution_failed",
                    Some(json!({"error": e.to_string()})),
                ))
            }
        }
    }
}

impl ServerHandler for AhmaMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "ahma_mcp".to_string(),
                version: "0.1.0".to_string(),
            },
            instructions: Some(
                "Ahma MCP: dynamic CLI tools over MCP.\n\n- Tools are exposed as '<tool>_<subcommand>' or '<tool>_run' if the tool has no subcommands (e.g., 'ls_run').\n- All tools require 'working_directory' (absolute path).\n- Pass flags as booleans (e.g., 'R': true) or via 'args': [\"-R\"]. You can also provide extra positional 'args'.\n- For long-running commands (build/test/bench), set 'enable_async_notification': true to avoid blocking and receive progress.\n\nExamples:\n- List recursively with ls: call 'ls_run' with { working_directory: '/abs/path', args: [\"-R\", \"-a\", \"-1\"] }\n- Run cargo build: call 'cargo_build' with { working_directory: '/abs/path', release: true, enable_async_notification: true }\n\nUse list_tools to discover available tools and their input schema."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self.tools_config.read().await;
        let mut mcp_tools = Vec::new();

        for (tool_name, (config, cli_structure)) in tools.iter() {
            info!("Processing tool: {}", tool_name);

            // Get base command info
            let mut base_description = format!("{} command-line tool", config.tool_name);
            // Append usage hint if available
            if let Some(hints) = &config.hints
                && let Some(usage) = &hints.usage
            {
                base_description = format!("{} | Usage: {}", base_description, usage.trim());
            }

            // Create a tool for each subcommand in the CLI structure
            if !cli_structure.subcommands.is_empty() {
                for subcommand in &cli_structure.subcommands {
                    let tool_id = format!("{}_{}", tool_name, subcommand.name);

                    let mut description =
                        format!("{}: {}", base_description, subcommand.description);
                    // Encourage async for long-running subcommands
                    let name_l = subcommand.name.to_lowercase();
                    if ["build", "test", "bench", "run", "doc", "clippy", "nextest"]
                        .iter()
                        .any(|k| name_l.contains(k))
                    {
                        description.push_str(" | Tip: set 'enable_async_notification': true for non-blocking execution.");
                    }

                    let input_schema = Self::cli_options_to_schema(&subcommand.options);

                    mcp_tools.push(Tool {
                        name: Cow::Owned(tool_id),
                        description: Some(Cow::Owned(description)),
                        input_schema,
                        annotations: None,
                        output_schema: None,
                    });
                }
            } else {
                // Single command tool - use global options
                let description = base_description.clone();
                let input_schema = Self::cli_options_to_schema(&cli_structure.global_options);

                mcp_tools.push(Tool {
                    name: Cow::Owned(format!("{}_run", tool_name)),
                    description: Some(Cow::Owned(description)),
                    input_schema,
                    annotations: None,
                    output_schema: None,
                });
            }
        }

        info!("Listing {} tools", mcp_tools.len());
        Ok(ListToolsResult {
            tools: mcp_tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        CallToolRequestParam { name, arguments }: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args_map = arguments.unwrap_or_else(JsonObject::new);
        let args_value = Value::Object(args_map);
        self.execute_tool(&name, args_value, ctx).await
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
