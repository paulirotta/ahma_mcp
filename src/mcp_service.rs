use std::{borrow::Cow, collections::HashMap, sync::Arc};

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, model::*,
    service::RequestContext, transport::stdio,
};
use serde_json::{Map, Value, json};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::{
    adapter::Adapter,
    cli_parser::{CliOption, CliStructure},
    config::Config,
};

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
        if options.is_empty() {
            // Return empty object schema when no options
            let mut schema = Map::new();
            schema.insert("type".to_string(), json!("object"));
            schema.insert("properties".to_string(), json!({}));
            return Arc::new(schema);
        }

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
            instructions: Some("Universal MCP adapter for CLI tools. This server dynamically exposes command-line tools as MCP tools based on TOML configurations.".to_string()),
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
            let base_description = format!("{} command-line tool", config.tool_name);

            // Create a tool for each subcommand in the CLI structure
            if !cli_structure.subcommands.is_empty() {
                for subcommand in &cli_structure.subcommands {
                    let tool_id = format!("{}_{}", tool_name, subcommand.name);

                    let description = format!("{}: {}", base_description, subcommand.description);

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
