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
//! - **`get_info()`**: Provides the client with initial information about the server.
//!
//! - **`list_tools()`**: This is the heart of the dynamic tool discovery mechanism. It
//!   iterates through the `tools_config` map and generates a `Tool` definition for each
//!   subcommand of each configured CLI tool.
//!
//! - **`call_tool()`**: This method handles the execution of a tool.
//!
//! ## Server Startup
//!
//! The `start_server()` method provides a convenient way to launch the service, wiring it
//! up to a standard I/O transport (`stdio`) and running it until completion.

use rmcp::{
    handler::server::ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, CancelledNotificationParam, Content,
        ErrorData as McpError, Implementation, ListToolsResult, PaginatedRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo, Tool, ToolsCapability,
    },
    service::{NotificationContext, Peer, RequestContext, RoleServer},
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::env;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::time::Instant;
use tracing;

use crate::{
    adapter::Adapter,
    callback_system::CallbackSender,
    config::{CommandOption, SequenceStep, SubcommandConfig, ToolConfig},
    constants::SEQUENCE_STEP_DELAY_MS,
    mcp_callback::McpCallbackSender,
    operation_monitor::Operation,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Represents the structure of the guidance JSON file.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct GuidanceConfig {
    pub guidance_blocks: HashMap<String, String>,
    #[serde(default)]
    pub templates: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub legacy_guidance: Option<LegacyGuidanceConfig>,
}

/// Legacy guidance config structure for backward compatibility
#[derive(serde::Deserialize, Debug, Clone)]
pub struct LegacyGuidanceConfig {
    pub general_guidance: HashMap<String, String>,
    pub tool_specific_guidance: HashMap<String, HashMap<String, String>>,
}

/// `AhmaMcpService` is the server handler for the MCP service.
#[derive(Clone)]
pub struct AhmaMcpService {
    pub adapter: Arc<Adapter>,
    pub operation_monitor: Arc<crate::operation_monitor::OperationMonitor>,
    pub configs: Arc<HashMap<String, ToolConfig>>,
    pub guidance: Arc<Option<GuidanceConfig>>,
    pub force_asynchronous: bool,
    /// The peer handle for sending notifications to the client.
    /// This is populated by capturing it from the first request context.
    pub peer: Arc<RwLock<Option<Peer<RoleServer>>>>,
}

impl AhmaMcpService {
    /// Creates a new `AhmaMcpService`.
    pub async fn new(
        adapter: Arc<Adapter>,
        operation_monitor: Arc<crate::operation_monitor::OperationMonitor>,
        configs: Arc<HashMap<String, ToolConfig>>,
        guidance: Arc<Option<GuidanceConfig>>,
        force_asynchronous: bool,
    ) -> Result<Self, anyhow::Error> {
        // Start the background monitor for operation timeouts
        crate::operation_monitor::OperationMonitor::start_background_monitor(
            operation_monitor.clone(),
        );

        Ok(Self {
            adapter,
            operation_monitor,
            configs,
            guidance,
            force_asynchronous,
            peer: Arc::new(RwLock::new(None)),
        })
    }

    /// Recursively collects all leaf subcommands from a tool's configuration.
    fn collect_leaf_subcommands<'a>(
        subcommands: &'a [SubcommandConfig],
        prefix: &str,
        leaves: &mut Vec<(String, &'a SubcommandConfig)>,
    ) {
        for sub in subcommands {
            if !sub.enabled {
                continue;
            }

            let current_path = if prefix.is_empty() {
                sub.name.clone()
            } else if sub.name == "default" {
                prefix.to_string()
            } else {
                format!("{}_{}", prefix, sub.name)
            };

            if let Some(nested_subcommands) = &sub.subcommand {
                Self::collect_leaf_subcommands(nested_subcommands, &current_path, leaves);
            } else {
                leaves.push((current_path, sub));
            }
        }
    }

    fn normalize_option_type(option_type: &str) -> &'static str {
        match option_type {
            "bool" | "boolean" => "boolean",
            "int" | "integer" => "integer",
            "array" => "array",
            "number" => "number",
            "string" => "string",
            _ => "string",
        }
    }

    fn build_items_schema(option: &CommandOption) -> Map<String, Value> {
        let mut items_schema = Map::new();

        if let Some(spec) = option.items.as_ref() {
            items_schema.insert("type".to_string(), Value::String(spec.item_type.clone()));
            if let Some(format) = &spec.format {
                items_schema.insert("format".to_string(), Value::String(format.clone()));
            }
            if let Some(description) = &spec.description {
                items_schema.insert(
                    "description".to_string(),
                    Value::String(description.clone()),
                );
            }
        } else {
            items_schema.insert("type".to_string(), Value::String("string".to_string()));
            if let Some(format) = &option.format {
                items_schema.insert("format".to_string(), Value::String(format.clone()));
            }
        }

        items_schema
    }

    /// Generates the JSON schema for a subcommand's options.
    fn get_schema_for_options(
        &self,
        sub_config: &SubcommandConfig,
    ) -> (Map<String, Value>, Vec<Value>) {
        let mut properties = Map::new();
        let mut required = Vec::new();

        if let Some(options) = sub_config.options.as_deref() {
            for option in options {
                let mut option_schema = Map::new();
                let param_type = Self::normalize_option_type(&option.option_type);
                option_schema.insert("type".to_string(), Value::String(param_type.to_string()));
                if param_type == "array" {
                    let items_schema = Self::build_items_schema(option);
                    option_schema.insert("items".to_string(), Value::Object(items_schema));
                }
                option_schema.insert(
                    "description".to_string(),
                    Value::String(option.description.clone().unwrap_or_default()),
                );
                if let Some(format) = &option.format {
                    option_schema.insert("format".to_string(), Value::String(format.clone()));
                }

                properties.insert(option.name.clone(), Value::Object(option_schema));

                if option.required.unwrap_or(false) {
                    required.push(Value::String(option.name.clone()));
                }
            }
        }

        if let Some(positional_args) = sub_config.positional_args.as_deref() {
            for arg in positional_args {
                let mut arg_schema = Map::new();
                let param_type = Self::normalize_option_type(&arg.option_type);
                arg_schema.insert("type".to_string(), Value::String(param_type.to_string()));
                if let Some(ref desc) = arg.description {
                    arg_schema.insert("description".to_string(), Value::String(desc.clone()));
                }
                if let Some(ref format) = arg.format {
                    arg_schema.insert("format".to_string(), Value::String(format.clone()));
                }
                if param_type == "array" {
                    let items_schema = Self::build_items_schema(arg);
                    arg_schema.insert("items".to_string(), Value::Object(items_schema));
                }
                properties.insert(arg.name.clone(), Value::Object(arg_schema));
                if arg.required.unwrap_or(false) {
                    required.push(Value::String(arg.name.clone()));
                }
            }
        }

        (properties, required)
    }

    /// Generates the JSON schema for a tool configuration file.
    fn generate_schema_for_tool_config(&self, tool_config: &ToolConfig) -> Arc<Map<String, Value>> {
        let mut leaf_subcommands = Vec::new();
        if let Some(subcommands) = &tool_config.subcommand {
            Self::collect_leaf_subcommands(subcommands, "", &mut leaf_subcommands);
        }

        // Case 1: Single default subcommand. No `subcommand` parameter needed.
        if leaf_subcommands.len() == 1 && leaf_subcommands[0].0 == "default" {
            let (_, sub_config) = &leaf_subcommands[0];
            let (mut properties, required) = self.get_schema_for_options(sub_config);

            if tool_config.name != "cargo" {
                properties.insert(
                    "working_directory".to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": "Working directory for command execution",
                        "format": "path"
                    }),
                );
            }

            let mut schema = Map::new();
            schema.insert("type".to_string(), Value::String("object".to_string()));
            schema.insert("properties".to_string(), Value::Object(properties));
            if !required.is_empty() {
                schema.insert("required".to_string(), Value::Array(required));
            }
            return Arc::new(schema);
        }

        // Case 2: Multiple subcommands. Use `subcommand` enum and `oneOf`.
        let mut all_properties = Map::new();
        let mut one_of = Vec::new();
        let mut subcommand_enum = Vec::new();

        for (path, sub_config) in leaf_subcommands {
            subcommand_enum.push(Value::String(path.clone()));
            let (sub_properties, sub_required) = self.get_schema_for_options(sub_config);

            // Merge properties into the main properties map
            all_properties.extend(sub_properties);

            let mut then_clause = Map::new();
            if !sub_required.is_empty() {
                then_clause.insert("required".to_string(), Value::Array(sub_required));
            }

            let if_clause = serde_json::json!({
                "properties": { "subcommand": { "const": path } }
            });

            let mut one_of_entry = Map::new();
            one_of_entry.insert("if".to_string(), if_clause);
            if !then_clause.is_empty() {
                one_of_entry.insert("then".to_string(), Value::Object(then_clause));
            }
            one_of.push(Value::Object(one_of_entry));
        }

        if !subcommand_enum.is_empty() {
            all_properties.insert(
                "subcommand".to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": "The subcommand to execute.",
                    "enum": subcommand_enum
                }),
            );
        }

        if tool_config.name != "cargo" {
            all_properties.insert(
                "working_directory".to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": "Working directory for command execution",
                    "format": "path"
                }),
            );
        }

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(all_properties));
        if !subcommand_enum.is_empty() {
            schema.insert(
                "required".to_string(),
                Value::Array(vec![Value::String("subcommand".to_string())]),
            );
        }
        if !one_of.is_empty() {
            schema.insert("oneOf".to_string(), Value::Array(one_of));
        }

        Arc::new(schema)
    }

    /// Creates an MCP Tool from a ToolConfig.
    fn create_tool_from_config(&self, tool_config: &ToolConfig) -> Tool {
        let tool_name = tool_config.name.clone();

        let mut description = tool_config.description.clone();
        if let Some(guidance_config) = self.guidance.as_ref()
            && let Some(guidance_text) = guidance_config.guidance_blocks.get(&tool_name)
        {
            description = format!("{}\n\n{}", guidance_text, description);
        }

        let input_schema = self.generate_schema_for_tool_config(tool_config);

        Tool {
            name: tool_name.into(),
            title: Some(tool_config.name.clone()),
            icons: None,
            description: Some(description.into()),
            input_schema,
            output_schema: None,
            annotations: None,
            meta: None,
        }
    }

    /// Finds the configuration for a subcommand from the tool arguments.
    fn find_subcommand_config_from_args<'a>(
        &self,
        tool_config: &'a ToolConfig,
        subcommand_name: Option<String>,
    ) -> Option<(&'a SubcommandConfig, Vec<String>)> {
        if !tool_config.enabled {
            tracing::warn!(
                "Attempted to resolve subcommand on disabled tool '{}'",
                tool_config.name
            );
            return None;
        }

        let subcommand_path = subcommand_name.unwrap_or_else(|| "default".to_string());
        let subcommand_parts: Vec<&str> = subcommand_path.split('_').collect();

        tracing::debug!(
            "Finding subcommand for tool '{}': path='{}', parts={:?}, has_subcommands={}",
            tool_config.name,
            subcommand_path,
            subcommand_parts,
            tool_config.subcommand.is_some()
        );

        let mut current_subcommands = tool_config.subcommand.as_ref()?;
        let mut found_subcommand: Option<&SubcommandConfig> = None;
        let mut command_parts = vec![tool_config.command.clone()];

        for (i, part) in subcommand_parts.iter().enumerate() {
            tracing::debug!(
                "Searching for subcommand part '{}' (index {}/{}) in {} candidates",
                part,
                i,
                subcommand_parts.len() - 1,
                current_subcommands.len()
            );

            if let Some(sub) = current_subcommands
                .iter()
                .find(|s| s.name == *part && s.enabled)
            {
                tracing::debug!(
                    "Found matching subcommand: name='{}', enabled={}",
                    sub.name,
                    sub.enabled
                );

                if sub.name != "default" {
                    command_parts.push(sub.name.clone());
                }

                if i == subcommand_parts.len() - 1 {
                    found_subcommand = Some(sub);
                    break;
                }

                if let Some(nested) = &sub.subcommand {
                    current_subcommands = nested;
                } else {
                    tracing::debug!(
                        "Subcommand '{}' has no nested subcommands, but path continues",
                        sub.name
                    );
                    return None; // More parts in name, but no more nested subcommands
                }
            } else {
                tracing::debug!(
                    "Subcommand part '{}' not found. Available: {:?}",
                    part,
                    current_subcommands
                        .iter()
                        .map(|s| &s.name)
                        .collect::<Vec<_>>()
                );
                return None; // Subcommand part not found
            }
        }

        found_subcommand.map(|sc| (sc, command_parts))
    }
}

#[async_trait::async_trait]
#[allow(clippy::manual_async_fn)] // Required by rmcp ServerHandler trait
impl ServerHandler for AhmaMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(true),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                title: Some(env!("CARGO_PKG_NAME").to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: None,
        }
    }

    fn on_initialized(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            tracing::info!("Client connected: {context:?}");
            // Get the peer from the context
            let peer = &context.peer;
            if self.peer.read().unwrap().is_none() {
                let mut peer_guard = self.peer.write().unwrap();
                if peer_guard.is_none() {
                    *peer_guard = Some(peer.clone());
                    tracing::info!(
                        "Successfully captured MCP peer handle for async notifications."
                    );
                }
            }
        }
    }

    fn on_cancelled(
        &self,
        notification: CancelledNotificationParam,
        _context: NotificationContext<RoleServer>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            let request_id = format!("{:?}", notification.request_id);
            let reason = notification
                .reason
                .as_deref()
                .unwrap_or("Client-initiated cancellation");

            tracing::info!(
                "MCP protocol cancellation received: request_id={}, reason='{}'",
                request_id,
                reason
            );

            // CRITICAL FIX: Only cancel background operations, not synchronous MCP calls
            // This prevents the rmcp library from generating "Canceled: Canceled" messages
            // that get incorrectly processed as process cancellations.

            let active_ops = self.operation_monitor.get_all_active_operations().await;
            let active_count = active_ops.len();

            if active_count > 0 {
                // Filter for operations that are actually background processes
                // vs. synchronous MCP tools like 'await' that don't have processes
                let background_ops: Vec<_> = active_ops
                    .iter()
                    .filter(|op| {
                        // Only cancel operations that represent actual background processes
                        // NOT synchronous tools like 'await', 'status', 'cancel'
                        !matches!(op.tool_name.as_str(), "await" | "status" | "cancel")
                    })
                    .collect();

                if !background_ops.is_empty() {
                    tracing::info!(
                        "Found {} background operations during MCP cancellation. Cancelling most recent background operation...",
                        background_ops.len()
                    );

                    if let Some(most_recent_bg_op) = background_ops.last() {
                        let enhanced_reason = format!(
                            "MCP protocol cancellation (request_id: {}, reason: '{}')",
                            request_id, reason
                        );

                        let cancelled = self
                            .operation_monitor
                            .cancel_operation_with_reason(
                                &most_recent_bg_op.id,
                                Some(enhanced_reason.clone()),
                            )
                            .await;

                        if cancelled {
                            tracing::info!(
                                "Successfully cancelled background operation '{}' due to MCP protocol cancellation: {}",
                                most_recent_bg_op.id,
                                enhanced_reason
                            );
                        } else {
                            tracing::warn!(
                                "Failed to cancel background operation '{}' for MCP protocol cancellation",
                                most_recent_bg_op.id
                            );
                        }
                    }
                } else {
                    tracing::info!(
                        "Found {} operations during MCP cancellation, but none are background processes. No cancellation needed.",
                        active_count
                    );
                }
            } else {
                tracing::info!(
                    "No active operations found during MCP protocol cancellation (request_id: {})",
                    request_id
                );
            }
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let mut tools = Vec::new();

            // Hard-wired await command - always available
            tools.push(Tool {
                name: "await".into(),
                title: Some("await".to_string()),
                icons: None,
                description: Some("Wait for previously started asynchronous operations to complete. **WARNING:** This is a blocking tool and makes you inefficient. **ONLY** use this if you have NO other tasks and cannot proceed until completion. It is **ALWAYS** better to perform other work and let results be pushed to you. **IMPORTANT:** Operations automatically notify you when complete - you do NOT need to check status repeatedly. Use this tool only when you genuinely cannot make progress without the results.".into()),
                input_schema: self.generate_input_schema_for_wait(),
                output_schema: None,
                annotations: None,
                meta: None,
            });

            // Hard-wired status command - always available
            tools.push(Tool {
                name: "status".into(),
                title: Some("status".to_string()),
                icons: None,
                description: Some("Query the status of operations without blocking. Shows active and completed operations. **IMPORTANT:** Results are automatically pushed to you when operations complete - you do NOT need to poll this tool repeatedly! If you find yourself calling 'status' multiple times for the same operation, you should use 'await' instead. Repeated status checks are an anti-pattern that wastes resources.".into()),
                input_schema: self.generate_input_schema_for_status(),
                output_schema: None,
                annotations: None,
                meta: None,
            });

            for config in self.configs.values() {
                if !config.enabled {
                    tracing::debug!("Skipping disabled tool '{}' during list_tools", config.name);
                    continue;
                }

                let tool = self.create_tool_from_config(config);
                tools.push(tool);
            }

            Ok(ListToolsResult {
                tools,
                next_cursor: None,
            })
        }
    }

    fn call_tool(
        &self,
        params: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let tool_name = params.name.as_ref();

            if tool_name == "status" {
                let args = params.arguments.unwrap_or_default();

                // Parse tools parameter as comma-separated string
                let tool_filters: Vec<String> = if let Some(v) = args.get("tools") {
                    if let Some(s) = v.as_str() {
                        s.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                // Parse operation_id parameter
                let specific_operation_id: Option<String> =
                    if let Some(v) = args.get("operation_id") {
                        v.as_str().map(|s| s.to_string())
                    } else {
                        None
                    };

                let mut contents = Vec::new();

                // Get active operations
                let active_ops: Vec<Operation> = self
                    .operation_monitor
                    .get_all_active_operations()
                    .await
                    .into_iter()
                    .filter(|op| {
                        let matches_filter = if tool_filters.is_empty() {
                            true
                        } else {
                            tool_filters.iter().any(|tn| op.tool_name.starts_with(tn))
                        };

                        let matches_id = if let Some(ref id) = specific_operation_id {
                            op.id == *id
                        } else {
                            true
                        };

                        matches_filter && matches_id
                    })
                    .collect();

                // Get completed operations
                let completed_ops: Vec<Operation> = self
                    .operation_monitor
                    .get_completed_operations()
                    .await
                    .into_iter()
                    .filter(|op| {
                        let matches_filter = if tool_filters.is_empty() {
                            true
                        } else {
                            tool_filters.iter().any(|tn| op.tool_name.starts_with(tn))
                        };

                        let matches_id = if let Some(ref id) = specific_operation_id {
                            op.id == *id
                        } else {
                            true
                        };

                        matches_filter && matches_id
                    })
                    .collect();

                // Create summary with timing information
                let active_count = active_ops.len();
                let completed_count = completed_ops.len();
                let total_count = active_count + completed_count;

                let summary = if let Some(ref id) = specific_operation_id {
                    if total_count == 0 {
                        format!("Operation '{}' not found", id)
                    } else {
                        format!("Operation '{}' found", id)
                    }
                } else if tool_filters.is_empty() {
                    format!(
                        "Operations status: {} active, {} completed (total: {})",
                        active_count, completed_count, total_count
                    )
                } else {
                    format!(
                        "Operations status for '{}': {} active, {} completed (total: {})",
                        tool_filters.join(", "),
                        active_count,
                        completed_count,
                        total_count
                    )
                };

                contents.push(Content::text(summary));

                // Add concurrency efficiency analysis
                if !completed_ops.is_empty() {
                    let mut total_execution_time = 0.0;
                    let mut total_wait_time = 0.0;
                    let mut operations_with_waits = 0;

                    for op in &completed_ops {
                        if let Some(end_time) = op.end_time
                            && let Ok(execution_duration) = end_time.duration_since(op.start_time)
                        {
                            total_execution_time += execution_duration.as_secs_f64();

                            if let Some(first_wait_time) = op.first_wait_time
                                && let Ok(wait_duration) =
                                    first_wait_time.duration_since(op.start_time)
                            {
                                total_wait_time += wait_duration.as_secs_f64();
                                operations_with_waits += 1;
                            }
                        }
                    }

                    if total_execution_time > 0.0 {
                        let efficiency_analysis = if operations_with_waits > 0 {
                            let avg_wait_ratio = (total_wait_time / total_execution_time) * 100.0;
                            if avg_wait_ratio < 10.0 {
                                format!(
                                    "✓ Good concurrency efficiency: {:.1}% of execution time spent waiting",
                                    avg_wait_ratio
                                )
                            } else if avg_wait_ratio < 50.0 {
                                format!(
                                    "⚠ Moderate concurrency efficiency: {:.1}% of execution time spent waiting",
                                    avg_wait_ratio
                                )
                            } else {
                                format!(
                                    "⚠ Low concurrency efficiency: {:.1}% of execution time spent waiting. Consider using status tool instead of frequent waits.",
                                    avg_wait_ratio
                                )
                            }
                        } else {
                            "✓ Excellent concurrency: No blocking waits detected".to_string()
                        };

                        contents.push(Content::text(format!(
                            "\nConcurrency Analysis:\n{}",
                            efficiency_analysis
                        )));
                    }
                }

                // Add active operations details
                if !active_ops.is_empty() {
                    contents.push(Content::text("\n=== ACTIVE OPERATIONS ===".to_string()));
                    for op in active_ops {
                        match serde_json::to_string_pretty(&op) {
                            Ok(s) => contents.push(Content::text(s)),
                            Err(e) => tracing::error!("Serialization error: {}", e),
                        }
                    }
                }

                // Add completed operations details
                if !completed_ops.is_empty() {
                    contents.push(Content::text("\n=== COMPLETED OPERATIONS ===".to_string()));
                    for op in completed_ops {
                        match serde_json::to_string_pretty(&op) {
                            Ok(s) => contents.push(Content::text(s)),
                            Err(e) => tracing::error!("Serialization error: {}", e),
                        }
                    }
                }

                return Ok(CallToolResult::success(contents));
            }

            if tool_name == "await" {
                return self.handle_await(params).await;
            }

            if tool_name == "cancel" {
                let args = params.arguments.unwrap_or_default();

                // Parse operation_id parameter (required)
                let operation_id = if let Some(v) = args.get("operation_id") {
                    if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        return Err(McpError::invalid_params(
                            "operation_id must be a string".to_string(),
                            Some(serde_json::json!({ "operation_id": v })),
                        ));
                    }
                } else {
                    return Err(McpError::invalid_params(
                        "operation_id parameter is required".to_string(),
                        Some(serde_json::json!({ "missing_param": "operation_id" })),
                    ));
                };

                // Optional cancellation reason to aid debugging
                let reason: Option<String> = args
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Attempt to cancel the operation
                let cancelled = self
                    .operation_monitor
                    .cancel_operation_with_reason(&operation_id, reason.clone())
                    .await;

                let result_message = if cancelled {
                    let why = reason
                        .as_deref()
                        .unwrap_or("No reason provided (default: user-initiated)");
                    format!(
                        "✓ Operation '{}' has been cancelled successfully.\nString: reason='{}'.\nHint: Consider restarting the operation if needed.",
                        operation_id, why
                    )
                } else {
                    // Check if operation exists but is already terminal
                    if let Some(operation) =
                        self.operation_monitor.get_operation(&operation_id).await
                    {
                        format!(
                            "⚠ Operation '{}' is already {} and cannot be cancelled.",
                            operation_id,
                            match operation.state {
                                crate::operation_monitor::OperationStatus::Completed => "completed",
                                crate::operation_monitor::OperationStatus::Failed => "failed",
                                crate::operation_monitor::OperationStatus::Cancelled => "cancelled",
                                crate::operation_monitor::OperationStatus::TimedOut => "timed out",
                                _ => "in a terminal state",
                            }
                        )
                    } else {
                        format!(
                            "❌ Operation '{}' not found. It may have already completed or never existed.",
                            operation_id
                        )
                    }
                };

                // Add a machine-parseable suggestion block to encourage restart via tool hint
                let suggestion = serde_json::json!({
                    "tool_hint": {
                        "suggested_tool": "status",
                        "reason": "Operation cancelled; check status and consider restarting",
                        "next_steps": [
                            {"tool": "status", "args": {"operation_id": operation_id}},
                            {"tool": "await", "args": {"tools": "", "timeout_seconds": 360}}
                        ]
                    }
                });

                return Ok(CallToolResult::success(vec![
                    Content::text(result_message),
                    Content::text(suggestion.to_string()),
                ]));
            }

            // Find tool configuration
            let config = match self.configs.get(tool_name) {
                Some(config) => config,
                None => {
                    let error_message = format!("Tool '{}' not found.", tool_name);
                    tracing::error!("{}", error_message);
                    return Err(McpError::invalid_params(
                        error_message,
                        Some(serde_json::json!({ "tool_name": tool_name })),
                    ));
                }
            };

            if !config.enabled {
                let error_message = format!(
                    "Tool '{}' is unavailable because its runtime availability probe failed",
                    tool_name
                );
                tracing::error!("{}", error_message);
                return Err(McpError::invalid_request(error_message, None));
            }

            // Check if this is a sequence tool
            if config.sequence.is_some() {
                return self.handle_sequence_tool(config, params, context).await;
            }

            let mut arguments = params.arguments.clone().unwrap_or_default();
            let subcommand_name = arguments
                .remove("subcommand")
                .and_then(|v| v.as_str().map(|s| s.to_string()));

            // Find the subcommand config and construct the command parts
            let (subcommand_config, command_parts) = match self
                .find_subcommand_config_from_args(config, subcommand_name.clone())
            {
                Some(result) => result,
                None => {
                    let has_subcommands = config.subcommand.is_some();
                    let num_subcommands = config.subcommand.as_ref().map(|s| s.len()).unwrap_or(0);
                    let subcommand_names: Vec<String> = config
                        .subcommand
                        .as_ref()
                        .map(|subs| {
                            subs.iter()
                                .map(|s| format!("{} (enabled={})", s.name, s.enabled))
                                .collect()
                        })
                        .unwrap_or_default();

                    let error_message = format!(
                        "Subcommand '{:?}' for tool '{}' not found or invalid. Tool enabled={}, has_subcommands={}, num_subcommands={}, available_subcommands={:?}",
                        subcommand_name,
                        tool_name,
                        config.enabled,
                        has_subcommands,
                        num_subcommands,
                        subcommand_names
                    );
                    tracing::error!("{}", error_message);
                    return Err(McpError::invalid_params(
                        error_message,
                        Some(
                            serde_json::json!({ "tool_name": tool_name, "subcommand": subcommand_name }),
                        ),
                    ));
                }
            };

            // Check if the subcommand itself is a sequence
            if subcommand_config.sequence.is_some() {
                return self
                    .handle_subcommand_sequence(config, subcommand_config, params, context)
                    .await;
            }

            let base_command = command_parts.join(" ");

            let working_directory = arguments
                .get("working_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            let timeout = arguments.get("timeout_seconds").and_then(|v| v.as_u64());

            // Determine execution mode (default is synchronous):
            // 1. If force_synchronous is true in config (subcommand or inherited from tool), ALWAYS use sync
            // 2. If --async CLI flag was used (force_asynchronous=true), use async mode
            // 3. Check explicit execution_mode argument (for advanced use)
            // 4. Default to synchronous
            //
            // Inheritance: subcommand.force_synchronous overrides tool.force_synchronous
            // If subcommand doesn't specify, inherit from tool level
            let force_sync = subcommand_config
                .force_synchronous
                .or(config.force_synchronous);
            let execution_mode = if force_sync == Some(true) {
                // Config explicitly requires synchronous: FORCE sync mode, ignoring CLI --async flag
                crate::adapter::ExecutionMode::Synchronous
            } else if self.force_asynchronous {
                // --async flag was used and not overridden by config: use async mode
                crate::adapter::ExecutionMode::AsyncResultPush
            } else if let Some(mode_str) = arguments.get("execution_mode").and_then(|v| v.as_str())
            {
                match mode_str {
                    "Synchronous" => crate::adapter::ExecutionMode::Synchronous,
                    "AsyncResultPush" => crate::adapter::ExecutionMode::AsyncResultPush,
                    _ => crate::adapter::ExecutionMode::Synchronous, // Default to sync
                }
            } else {
                // Default to synchronous mode
                crate::adapter::ExecutionMode::Synchronous
            };

            match execution_mode {
                crate::adapter::ExecutionMode::Synchronous => {
                    let result = self
                        .adapter
                        .execute_sync_in_dir(
                            &base_command,
                            Some(arguments),
                            &working_directory,
                            timeout,
                            Some(subcommand_config),
                        )
                        .await;

                    match result {
                        Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
                        Err(e) => {
                            let error_message = format!("Synchronous execution failed: {}", e);
                            tracing::error!("{}", error_message);
                            Err(McpError::internal_error(error_message, None))
                        }
                    }
                }
                crate::adapter::ExecutionMode::AsyncResultPush => {
                    let operation_id = format!("op_{}", NEXT_ID.fetch_add(1, Ordering::SeqCst));
                    let callback: Box<dyn CallbackSender> = Box::new(McpCallbackSender::new(
                        context.peer.clone(),
                        operation_id.clone(),
                    ));

                    let job_id = self
                        .adapter
                        .execute_async_in_dir_with_options(
                            tool_name,
                            &base_command,
                            &working_directory,
                            crate::adapter::AsyncExecOptions {
                                operation_id: Some(operation_id),
                                args: Some(arguments),
                                timeout,
                                callback: Some(callback),
                                subcommand_config: Some(subcommand_config),
                            },
                        )
                        .await;

                    match job_id {
                        Ok(id) => {
                            // Include tool hints to guide AI on handling async operations
                            let hint = crate::tool_hints::preview(&id, tool_name);
                            let message =
                                format!("Asynchronous operation started with ID: {}{}", id, hint);
                            Ok(CallToolResult::success(vec![Content::text(message)]))
                        }
                        Err(e) => {
                            let error_message =
                                format!("Failed to start asynchronous operation: {}", e);
                            tracing::error!("{}", error_message);
                            Err(McpError::internal_error(error_message, None))
                        }
                    }
                }
            }
        }
    }
}

impl AhmaMcpService {
    /// Generates the specific input schema for the `await` tool.
    fn generate_input_schema_for_wait(&self) -> Arc<Map<String, Value>> {
        let mut properties = Map::new();
        properties.insert(
            "tools".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Comma-separated tool name prefixes to await for (optional; waits for all if omitted)"
            }),
        );
        properties.insert(
            "operation_id".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Specific operation ID to await for (optional)"
            }),
        );

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(properties));
        Arc::new(schema)
    }

    /// Generates the specific input schema for the `status` tool.
    fn generate_input_schema_for_status(&self) -> Arc<Map<String, Value>> {
        let mut properties = Map::new();
        properties.insert(
            "tools".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Comma-separated tool name prefixes to filter by (optional; shows all if omitted)"
            }),
        );
        properties.insert(
            "operation_id".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Specific operation ID to query (optional; shows all if omitted)"
            }),
        );

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(properties));
        Arc::new(schema)
    }

    /// Handles execution of sequence tools - tools that invoke multiple other tools in order.
    async fn handle_sequence_tool(
        &self,
        config: &crate::config::ToolConfig,
        params: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let sequence = config.sequence.as_ref().unwrap(); // Safe due to prior check
        let step_delay_ms = config.step_delay_ms.unwrap_or(SEQUENCE_STEP_DELAY_MS);

        // Determine if sequence should run synchronously
        // - If force_synchronous is true, always run sync
        // - If force_synchronous is false or None, default is async for sequence tools
        let run_synchronously = config.force_synchronous.unwrap_or(false);

        if run_synchronously {
            self.handle_sequence_tool_sync(config, params, context, sequence, step_delay_ms)
                .await
        } else {
            self.handle_sequence_tool_async(config, params, context, sequence, step_delay_ms)
                .await
        }
    }

    /// Handles synchronous sequence execution - blocks until all steps complete
    async fn handle_sequence_tool_sync(
        &self,
        config: &crate::config::ToolConfig,
        params: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
        sequence: &[SequenceStep],
        step_delay_ms: u64,
    ) -> Result<CallToolResult, McpError> {
        let mut final_result = CallToolResult::success(vec![]);
        let mut all_outputs = Vec::new();

        for (index, step) in sequence.iter().enumerate() {
            if Self::should_skip_sequence_tool_step(&step.tool) {
                let message = Self::format_sequence_step_skipped_message(step);
                final_result.content.push(Content::text(message));
                continue;
            }

            // Extract working directory from parent args
            let parent_args = params.arguments.clone().unwrap_or_default();
            let working_directory = parent_args
                .get("working_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            // Merge arguments, excluding meta-parameters
            let mut merged_args = Map::new();
            for (key, value) in parent_args.iter() {
                if key != "working_directory" && key != "execution_mode" && key != "timeout_seconds"
                {
                    merged_args.insert(key.clone(), value.clone());
                }
            }
            merged_args.extend(step.args.clone());

            let step_tool_config = match self.configs.get(&step.tool) {
                Some(cfg) => cfg,
                None => {
                    let error_message = format!(
                        "Tool '{}' referenced in sequence step is not configured.",
                        step.tool
                    );
                    return Err(McpError::internal_error(error_message, None));
                }
            };

            let (subcommand_config, command_parts) = match self
                .find_subcommand_config_from_args(step_tool_config, Some(step.subcommand.clone()))
            {
                Some(result) => result,
                None => {
                    let error_message = format!(
                        "Subcommand '{}' for tool '{}' not found in sequence step.",
                        step.subcommand, step.tool
                    );
                    return Err(McpError::internal_error(error_message, None));
                }
            };

            // Execute synchronously and wait for result
            let step_result = self
                .adapter
                .execute_sync_in_dir(
                    &command_parts.join(" "),
                    Some(merged_args),
                    &working_directory,
                    config.timeout_seconds,
                    Some(subcommand_config),
                )
                .await;

            match step_result {
                Ok(output) => {
                    let message = format!(
                        "✓ Step {} completed: {} {}\n{}",
                        index + 1,
                        step.tool,
                        step.subcommand,
                        if output.is_empty() {
                            "(no output)"
                        } else {
                            &output
                        }
                    );
                    all_outputs.push(message.clone());
                    tracing::info!(
                        "Sequence step {} succeeded: {} {}",
                        index + 1,
                        step.tool,
                        step.subcommand
                    );
                }
                Err(e) => {
                    let error_message = format!(
                        "✗ Step {} FAILED: {} {}\nError: {}",
                        index + 1,
                        step.tool,
                        step.subcommand,
                        e
                    );
                    all_outputs.push(error_message.clone());
                    tracing::error!(
                        "Sequence step {} failed: {} {}: {}",
                        index + 1,
                        step.tool,
                        step.subcommand,
                        e
                    );

                    // Return failure immediately - don't continue with remaining steps
                    final_result.content.push(Content::text(format!(
                        "Sequence failed at step {}:\n\n{}",
                        index + 1,
                        all_outputs.join("\n\n")
                    )));
                    final_result.is_error = Some(true);
                    return Ok(final_result);
                }
            }

            // Add delay between steps
            if step_delay_ms > 0 && index + 1 < sequence.len() {
                tokio::time::sleep(Duration::from_millis(step_delay_ms)).await;
            }
        }

        // All steps succeeded
        final_result.content.push(Content::text(format!(
            "All {} sequence steps completed successfully:\n\n{}",
            sequence.len(),
            all_outputs.join("\n\n")
        )));
        Ok(final_result)
    }

    /// Handles asynchronous sequence execution - starts all steps and returns immediately
    async fn handle_sequence_tool_async(
        &self,
        _config: &crate::config::ToolConfig,
        params: CallToolRequestParam,
        context: RequestContext<RoleServer>,
        sequence: &[SequenceStep],
        step_delay_ms: u64,
    ) -> Result<CallToolResult, McpError> {
        let mut final_result = CallToolResult::success(vec![]);

        for (index, step) in sequence.iter().enumerate() {
            if Self::should_skip_sequence_tool_step(&step.tool) {
                let message = Self::format_sequence_step_skipped_message(step);
                final_result.content.push(Content::text(message));
                continue;
            }
            let mut step_params = params.clone();
            step_params.name = step.tool.clone().into();

            // Extract meta-parameters that should not be passed to tools
            let parent_args = params.arguments.clone().unwrap_or_default();
            let working_directory = parent_args
                .get("working_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            // Merge arguments, excluding meta-parameters from parent
            let mut merged_args = Map::new();
            for (key, value) in parent_args.iter() {
                // Skip meta-parameters that are handled separately
                if key != "working_directory" && key != "execution_mode" && key != "timeout_seconds"
                {
                    merged_args.insert(key.clone(), value.clone());
                }
            }
            // Add working_directory back for the step to use
            merged_args.insert(
                "working_directory".to_string(),
                Value::String(working_directory),
            );
            // Extend with step-specific args (which can override)
            merged_args.extend(step.args.clone());
            step_params.arguments = Some(merged_args);

            let step_tool_config = match self.configs.get(&step.tool) {
                Some(cfg) => cfg,
                None => {
                    let error_message = format!(
                        "Tool '{}' referenced in sequence step is not configured.",
                        step.tool
                    );
                    return Err(McpError::internal_error(error_message, None));
                }
            };

            let (subcommand_config, command_parts) = match self
                .find_subcommand_config_from_args(step_tool_config, Some(step.subcommand.clone()))
            {
                Some(result) => result,
                None => {
                    let error_message = format!(
                        "Subcommand '{}' for tool '{}' not found in sequence step.",
                        step.subcommand, step.tool
                    );
                    return Err(McpError::internal_error(error_message, None));
                }
            };

            let operation_id = format!("op_{}", NEXT_ID.fetch_add(1, Ordering::SeqCst));
            let callback: Box<dyn CallbackSender> = Box::new(McpCallbackSender::new(
                context.peer.clone(),
                operation_id.clone(),
            ));

            let step_result = self
                .adapter
                .execute_async_in_dir_with_options(
                    &step.tool,
                    &command_parts.join(" "),
                    ".", // Assuming current directory for now
                    crate::adapter::AsyncExecOptions {
                        operation_id: Some(operation_id),
                        args: step_params.arguments,
                        timeout: None, // Add timeout logic if needed
                        callback: Some(callback),
                        subcommand_config: Some(subcommand_config),
                    },
                )
                .await;

            match step_result {
                Ok(id) => {
                    let message = Self::format_sequence_step_message(step, &id);
                    final_result.content.push(Content::text(message));
                }
                Err(e) => {
                    let error_message = format!(
                        "Sequence step '{}' failed to start: {}. Halting sequence.",
                        step.tool, e
                    );
                    tracing::error!("{}", error_message);
                    return Err(McpError::internal_error(error_message, None));
                }
            }

            if step_delay_ms > 0 && index + 1 < sequence.len() {
                tokio::time::sleep(Duration::from_millis(step_delay_ms)).await;
            }
        }

        Ok(final_result)
    }

    /// Handles execution of subcommand sequences - subcommands that invoke multiple cargo commands in order.
    async fn handle_subcommand_sequence(
        &self,
        config: &crate::config::ToolConfig,
        subcommand_config: &crate::config::SubcommandConfig,
        params: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let sequence: &Vec<SequenceStep> = subcommand_config.sequence.as_ref().unwrap(); // Safe due to prior check
        let step_delay_ms = subcommand_config
            .step_delay_ms
            .or(config.step_delay_ms)
            .unwrap_or(SEQUENCE_STEP_DELAY_MS);
        let mut final_result = CallToolResult::success(vec![]);

        for (index, step) in sequence.iter().enumerate() {
            if Self::should_skip_sequence_subcommand_step(&step.subcommand) {
                let message = Self::format_subcommand_sequence_step_skipped_message(step);
                final_result.content.push(Content::text(message));
                continue;
            }
            let (step_config, command_parts) = match self
                .find_subcommand_config_from_args(config, Some(step.subcommand.clone()))
            {
                Some(result) => result,
                None => {
                    let error_message = format!(
                        "Subcommand sequence step '{}' not found in tool config. Halting sequence.",
                        step.subcommand
                    );
                    tracing::error!("{}", error_message);
                    return Err(McpError::internal_error(error_message, None));
                }
            };

            let operation_id = format!("op_{}", NEXT_ID.fetch_add(1, Ordering::SeqCst));
            let callback: Box<dyn CallbackSender> = Box::new(McpCallbackSender::new(
                context.peer.clone(),
                operation_id.clone(),
            ));

            let step_result = self
                .adapter
                .execute_async_in_dir_with_options(
                    &config.name,
                    &command_parts.join(" "),
                    ".",
                    crate::adapter::AsyncExecOptions {
                        operation_id: Some(operation_id),
                        args: params.arguments.clone(),
                        timeout: None,
                        callback: Some(callback),
                        subcommand_config: Some(step_config),
                    },
                )
                .await;

            match step_result {
                Ok(id) => {
                    let message = Self::format_subcommand_sequence_step_message(step, &id);
                    final_result.content.push(Content::text(message));
                }
                Err(e) => {
                    let error_message = format!(
                        "Subcommand sequence step '{}' failed to start: {}. Halting sequence.",
                        step.subcommand, e
                    );
                    tracing::error!("{}", error_message);
                    return Err(McpError::internal_error(error_message, None));
                }
            }

            if step_delay_ms > 0 && index + 1 < sequence.len() {
                tokio::time::sleep(Duration::from_millis(step_delay_ms)).await;
            }
        }

        Ok(final_result)
    }

    fn format_sequence_step_message(step: &SequenceStep, operation_id: &str) -> String {
        let hint = crate::tool_hints::preview(operation_id, &step.tool);
        match step.description.as_deref() {
            Some(description) if !description.is_empty() => format!(
                "Sequence step '{}' ({}) started with operation ID: {}{}",
                step.tool, description, operation_id, hint
            ),
            _ => format!(
                "Sequence step '{}' started with operation ID: {}{}",
                step.tool, operation_id, hint
            ),
        }
    }

    fn format_subcommand_sequence_step_message(step: &SequenceStep, operation_id: &str) -> String {
        let hint = crate::tool_hints::preview(operation_id, &step.subcommand);
        match step.description.as_deref() {
            Some(description) if !description.is_empty() => format!(
                "Subcommand sequence step '{}' ({}) started with ID: {}{}",
                step.subcommand, description, operation_id, hint
            ),
            _ => format!(
                "Subcommand sequence step '{}' started with ID: {}{}",
                step.subcommand, operation_id, hint
            ),
        }
    }

    fn format_sequence_step_skipped_message(step: &SequenceStep) -> String {
        match step.description.as_deref() {
            Some(description) if !description.is_empty() => format!(
                "Sequence step '{}' ({}) skipped due to environment override.",
                step.tool, description
            ),
            _ => format!(
                "Sequence step '{}' skipped due to environment override.",
                step.tool
            ),
        }
    }

    fn format_subcommand_sequence_step_skipped_message(step: &SequenceStep) -> String {
        match step.description.as_deref() {
            Some(description) if !description.is_empty() => format!(
                "Subcommand sequence step '{}' ({}) skipped due to environment override.",
                step.subcommand, description
            ),
            _ => format!(
                "Subcommand sequence step '{}' skipped due to environment override.",
                step.subcommand
            ),
        }
    }

    fn should_skip_sequence_tool_step(tool: &str) -> bool {
        Self::env_list_contains("AHMA_SKIP_SEQUENCE_TOOLS", tool)
    }

    fn should_skip_sequence_subcommand_step(subcommand: &str) -> bool {
        Self::env_list_contains("AHMA_SKIP_SEQUENCE_SUBCOMMANDS", subcommand)
    }

    fn env_list_contains(env_key: &str, value: &str) -> bool {
        match env::var(env_key) {
            Ok(list) => list
                .split(',')
                .map(|entry| entry.trim())
                .filter(|entry| !entry.is_empty())
                .any(|entry| entry.eq_ignore_ascii_case(value)),
            Err(_) => false,
        }
    }

    /// Handles the 'await' tool call.
    async fn handle_await(&self, params: CallToolRequestParam) -> Result<CallToolResult, McpError> {
        let args = params.arguments.unwrap_or_default();

        // Check if a specific operation_id is provided
        let operation_id_filter: Option<String> = args
            .get("operation_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse tools parameter as comma-separated string
        let tool_filters: Vec<String> = if let Some(v) = args.get("tools") {
            if let Some(s) = v.as_str() {
                s.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // If operation_id is specified, wait for that specific operation
        if let Some(op_id) = operation_id_filter {
            // Check if operation exists
            let operation = self.operation_monitor.get_operation(&op_id).await;

            if operation.is_none() {
                // Check if it's in completed operations
                let completed_ops = self.operation_monitor.get_completed_operations().await;
                if let Some(completed_op) = completed_ops.iter().find(|op| op.id == op_id) {
                    // Operation already completed
                    let mut contents = Vec::new();
                    contents.push(Content::text(format!(
                        "Operation {} already completed",
                        op_id
                    )));
                    match serde_json::to_string_pretty(completed_op) {
                        Ok(s) => contents.push(Content::text(s)),
                        Err(e) => tracing::error!("Serialization error: {}", e),
                    }
                    return Ok(CallToolResult::success(contents));
                } else {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "Operation {} not found",
                        op_id
                    ))]));
                }
            }

            // Wait for the specific operation
            tracing::info!("Waiting for operation: {}", op_id);

            // Use a reasonable timeout (e.g., 5 minutes)
            let timeout_duration = std::time::Duration::from_secs(300);
            let wait_start = Instant::now();

            let wait_result = tokio::time::timeout(
                timeout_duration,
                self.operation_monitor.wait_for_operation(&op_id),
            )
            .await;

            match wait_result {
                Ok(Some(completed_op)) => {
                    let elapsed = wait_start.elapsed();
                    let mut contents = vec![Content::text(format!(
                        "Completed 1 operations in {:.2}s",
                        elapsed.as_secs_f64()
                    ))];
                    match serde_json::to_string_pretty(&completed_op) {
                        Ok(s) => contents.push(Content::text(s)),
                        Err(e) => tracing::error!("Serialization error: {}", e),
                    }
                    Ok(CallToolResult::success(contents))
                }
                Ok(None) => Ok(CallToolResult::success(vec![Content::text(format!(
                    "Operation {} completed but no result available",
                    op_id
                ))])),
                Err(_) => Ok(CallToolResult::success(vec![Content::text(format!(
                    "Timeout waiting for operation {}",
                    op_id
                ))])),
            }
        } else {
            // Original behavior: wait for operations by tool filter
            // Always use intelligent timeout calculation (no user-provided timeout parameter)
            let timeout_seconds = self.calculate_intelligent_timeout(&tool_filters).await;
            let timeout_duration = std::time::Duration::from_secs(timeout_seconds as u64);

            // Build from pending ops, optionally filtered by tools
            let pending_ops: Vec<Operation> = self
                .operation_monitor
                .get_all_active_operations()
                .await
                .into_iter()
                .filter(|op| {
                    if op.state.is_terminal() {
                        return false;
                    }
                    if tool_filters.is_empty() {
                        true
                    } else {
                        tool_filters.iter().any(|tn| op.tool_name.starts_with(tn))
                    }
                })
                .collect();

            if pending_ops.is_empty() {
                let completed_ops = self.operation_monitor.get_completed_operations().await;
                let relevant_completed: Vec<Operation> = completed_ops
                    .into_iter()
                    .filter(|op| {
                        if tool_filters.is_empty() {
                            false
                        } else {
                            tool_filters.iter().any(|tn| op.tool_name.starts_with(tn))
                        }
                    })
                    .collect();

                if !relevant_completed.is_empty() {
                    let mut contents = Vec::new();
                    contents.push(Content::text(format!(
                    "No pending operations for tools: {}. However, these operations recently completed:",
                    tool_filters.join(", ")
                )));
                    for op in relevant_completed {
                        match serde_json::to_string_pretty(&op) {
                            Ok(s) => contents.push(Content::text(s)),
                            Err(e) => tracing::error!("Serialization error: {}", e),
                        }
                    }
                    return Ok(CallToolResult::success(contents));
                }

                return Ok(CallToolResult::success(vec![Content::text(
                    if tool_filters.is_empty() {
                        "No pending operations to await for.".to_string()
                    } else {
                        format!(
                            "No pending operations for tools: {}",
                            tool_filters.join(", ")
                        )
                    },
                )]));
            }

            tracing::info!(
                "Waiting for {} pending operations (timeout: {}s): {:?}",
                pending_ops.len(),
                timeout_seconds,
                pending_ops.iter().map(|op| &op.id).collect::<Vec<_>>()
            );

            let wait_start = Instant::now();
            let (warning_tx, mut warning_rx) = tokio::sync::mpsc::unbounded_channel();

            let warning_task = {
                let warning_tx = warning_tx.clone();
                let timeout_secs = timeout_seconds;
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.5))
                        .await;
                    let _ = warning_tx.send(format!(
                        "Wait operation 50% complete ({:.0}s remaining)",
                        timeout_secs * 0.5
                    ));
                    tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.25))
                        .await;
                    let _ = warning_tx.send(format!(
                        "Wait operation 75% complete ({:.0}s remaining)",
                        timeout_secs * 0.25
                    ));
                    tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.15))
                        .await;
                    let _ = warning_tx.send(format!(
                        "Wait operation 90% complete ({:.0}s remaining)",
                        timeout_secs * 0.1
                    ));
                })
            };

            let wait_result = tokio::time::timeout(timeout_duration, async {
                let mut contents = Vec::new();
                let mut futures = Vec::new();
                for op in &pending_ops {
                    futures.push(self.operation_monitor.wait_for_operation(&op.id));
                }

                let results = futures::future::join_all(futures).await;

                for done in results.into_iter().flatten() {
                    match serde_json::to_string_pretty(&done) {
                        Ok(s) => contents.push(Content::text(s)),
                        Err(e) => tracing::error!("Serialization error: {}", e),
                    }
                }
                contents
            })
            .await;

            warning_task.abort();
            while let Ok(warning) = warning_rx.try_recv() {
                tracing::info!("Wait progress: {}", warning);
            }

            match wait_result {
                Ok(contents) => {
                    let elapsed = wait_start.elapsed();
                    if !contents.is_empty() {
                        let mut result_contents = vec![Content::text(format!(
                            "Completed {} operations in {:.2}s",
                            contents.len(),
                            elapsed.as_secs_f64()
                        ))];

                        result_contents.extend(contents);
                        Ok(CallToolResult::success(result_contents))
                    } else {
                        let result_contents = vec![Content::text(
                            "No operations completed within timeout period".to_string(),
                        )];

                        Ok(CallToolResult::success(result_contents))
                    }
                }
                Err(_) => {
                    let elapsed = wait_start.elapsed();
                    let still_running: Vec<Operation> = self
                        .operation_monitor
                        .get_all_active_operations()
                        .await
                        .into_iter()
                        .filter(|op| !op.state.is_terminal())
                        .collect();
                    let completed_during_wait = pending_ops.len() - still_running.len();

                    let mut remediation_steps = Vec::new();
                    let lock_patterns = vec![
                        ".cargo-lock",
                        ".lock",
                        "package-lock.json",
                        "yarn.lock",
                        ".npm-lock",
                        "composer.lock",
                        "Pipfile.lock",
                        ".bundle-lock",
                    ];
                    for dir in &["target", "node_modules", ".cargo", "tmp", "temp"] {
                        if let Ok(entries) = std::fs::read_dir(dir) {
                            for entry in entries.flatten() {
                                if let Some(name) = entry.file_name().to_str() {
                                    for pattern in &lock_patterns {
                                        if name.contains(pattern) {
                                            remediation_steps.push(format!(
                                                "• Remove potential stale lock file: rm {}/{}",
                                                dir, name
                                            ));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if std::fs::metadata(".").is_ok() {
                        remediation_steps.push("• Check available disk space: df -h .".to_string());
                    }
                    let running_commands: std::collections::HashSet<String> = still_running
                        .iter()
                        .map(|op| {
                            op.tool_name
                                .split('_')
                                .next()
                                .unwrap_or(&op.tool_name)
                                .to_string()
                        })
                        .collect();
                    for cmd in &running_commands {
                        remediation_steps.push(format!(
                            "• Check for competing {} processes: ps aux | grep {}",
                            cmd, cmd
                        ));
                    }
                    let network_keywords = [
                        "network", "http", "https", "tcp", "udp", "socket", "curl", "wget", "git",
                        "api", "rest", "graphql", "rpc", "ssh", "ftp", "scp", "rsync", "net",
                        "audit", "update", "search", "add", "install", "fetch", "clone", "pull",
                        "push", "download", "upload", "sync",
                    ];
                    let has_network_ops = still_running.iter().any(|op| {
                        network_keywords
                            .iter()
                            .any(|keyword| op.tool_name.contains(keyword))
                    });
                    if has_network_ops {
                        remediation_steps.push(
                        "• Network operations detected - check internet connection: ping 8.8.8.8"
                            .to_string(),
                    );
                        remediation_steps.push(
                            "• Try running with offline flags if tool supports them".to_string(),
                        );
                    }
                    let build_keywords = [
                        "build", "compile", "test", "lint", "clippy", "format", "check", "verify",
                        "validate", "analyze",
                    ];
                    let has_build_ops = still_running.iter().any(|op| {
                        build_keywords
                            .iter()
                            .any(|keyword| op.tool_name.contains(keyword))
                    });
                    if has_build_ops {
                        remediation_steps.push("• Build/compile operations can take time - consider increasing timeout_seconds".to_string());
                        remediation_steps.push("• Check system resources: top or htop".to_string());
                        remediation_steps.push(
                            "• Consider running operations with verbose flags to see progress"
                                .to_string(),
                        );
                    }
                    if remediation_steps.is_empty() {
                        remediation_steps.push(
                            "• Use the 'status' tool to check remaining operations".to_string(),
                        );
                        remediation_steps.push(
                        "• Operations continue running in background - they may complete shortly"
                            .to_string(),
                    );
                        remediation_steps.push("• Consider increasing timeout_seconds if operations legitimately need more time".to_string());
                    }
                    let mut error_message = format!(
                        "Wait operation timed out after {:.2}s (configured timeout: {:.0}s).\n\n\
                    Progress: {}/{} operations completed during await.\n\
                    Still running: {} operations.\n\n\
                    Suggestions:",
                        elapsed.as_secs_f64(),
                        timeout_seconds,
                        completed_during_wait,
                        pending_ops.len(),
                        still_running.len()
                    );
                    for step in &remediation_steps {
                        error_message.push_str(&format!("\n{}", step));
                    }
                    if !still_running.is_empty() {
                        error_message.push_str("\n\nStill running operations:");
                        for op in &still_running {
                            error_message.push_str(&format!("\n• {} ({})", op.id, op.tool_name));
                        }
                    }
                    Ok(CallToolResult::success(vec![Content::text(error_message)]))
                }
            }
        } // Close the else block
    }

    /// Calculate intelligent timeout based on operation timeouts and default await timeout
    ///
    /// Returns the maximum of:
    /// 1. Default await timeout (240 seconds)
    /// 2. Maximum timeout of all pending operations (filtered by tool if specified)
    pub async fn calculate_intelligent_timeout(&self, tool_filters: &[String]) -> f64 {
        const DEFAULT_AWAIT_TIMEOUT: f64 = 240.0;

        let pending_ops = self.operation_monitor.get_all_active_operations().await;

        let max_op_timeout = pending_ops
            .iter()
            .filter(|op| {
                tool_filters.is_empty() || tool_filters.iter().any(|f| op.tool_name.starts_with(f))
            })
            .filter_map(|op| op.timeout_duration)
            .map(|t| t.as_secs_f64())
            .fold(0.0, f64::max);

        DEFAULT_AWAIT_TIMEOUT.max(max_op_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CommandOption, ItemsSpec, SubcommandConfig};

    // ============= normalize_option_type tests =============

    #[test]
    fn test_normalize_option_type_bool_variants() {
        assert_eq!(AhmaMcpService::normalize_option_type("bool"), "boolean");
        assert_eq!(AhmaMcpService::normalize_option_type("boolean"), "boolean");
    }

    #[test]
    fn test_normalize_option_type_int_variants() {
        assert_eq!(AhmaMcpService::normalize_option_type("int"), "integer");
        assert_eq!(AhmaMcpService::normalize_option_type("integer"), "integer");
    }

    #[test]
    fn test_normalize_option_type_passthrough() {
        assert_eq!(AhmaMcpService::normalize_option_type("array"), "array");
        assert_eq!(AhmaMcpService::normalize_option_type("number"), "number");
        assert_eq!(AhmaMcpService::normalize_option_type("string"), "string");
    }

    #[test]
    fn test_normalize_option_type_unknown_defaults_to_string() {
        assert_eq!(AhmaMcpService::normalize_option_type("unknown"), "string");
        assert_eq!(AhmaMcpService::normalize_option_type("foo"), "string");
        assert_eq!(AhmaMcpService::normalize_option_type(""), "string");
    }

    // ============= build_items_schema tests =============

    #[test]
    fn test_build_items_schema_without_items_spec() {
        let option = CommandOption {
            name: "files".to_string(),
            option_type: "array".to_string(),
            description: Some("List of files".to_string()),
            required: None,
            format: None,
            items: None,
            file_arg: None,
            file_flag: None,
            alias: None,
        };

        let schema = AhmaMcpService::build_items_schema(&option);
        assert_eq!(schema.get("type").unwrap(), "string");
        assert!(schema.get("format").is_none());
    }

    #[test]
    fn test_build_items_schema_with_format_on_option() {
        let option = CommandOption {
            name: "files".to_string(),
            option_type: "array".to_string(),
            description: Some("List of files".to_string()),
            required: None,
            format: Some("path".to_string()),
            items: None,
            file_arg: None,
            file_flag: None,
            alias: None,
        };

        let schema = AhmaMcpService::build_items_schema(&option);
        assert_eq!(schema.get("type").unwrap(), "string");
        assert_eq!(schema.get("format").unwrap(), "path");
    }

    #[test]
    fn test_build_items_schema_with_full_items_spec() {
        let option = CommandOption {
            name: "files".to_string(),
            option_type: "array".to_string(),
            description: Some("List of files".to_string()),
            required: None,
            format: None,
            items: Some(ItemsSpec {
                item_type: "string".to_string(),
                format: Some("path".to_string()),
                description: Some("A file path".to_string()),
            }),
            file_arg: None,
            file_flag: None,
            alias: None,
        };

        let schema = AhmaMcpService::build_items_schema(&option);
        assert_eq!(schema.get("type").unwrap(), "string");
        assert_eq!(schema.get("format").unwrap(), "path");
        assert_eq!(schema.get("description").unwrap(), "A file path");
    }

    #[test]
    fn test_build_items_schema_items_type_override() {
        // Items spec should override default string type
        let option = CommandOption {
            name: "ids".to_string(),
            option_type: "array".to_string(),
            description: Some("List of IDs".to_string()),
            required: None,
            format: None,
            items: Some(ItemsSpec {
                item_type: "integer".to_string(),
                format: None,
                description: None,
            }),
            file_arg: None,
            file_flag: None,
            alias: None,
        };

        let schema = AhmaMcpService::build_items_schema(&option);
        assert_eq!(schema.get("type").unwrap(), "integer");
    }

    // ============= collect_leaf_subcommands tests =============

    fn make_subcommand(name: &str, description: &str, enabled: bool) -> SubcommandConfig {
        SubcommandConfig {
            name: name.to_string(),
            description: description.to_string(),
            subcommand: None,
            options: None,
            positional_args: None,
            timeout_seconds: None,
            force_synchronous: None,
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
        description: &str,
        enabled: bool,
        nested: Vec<SubcommandConfig>,
    ) -> SubcommandConfig {
        SubcommandConfig {
            name: name.to_string(),
            description: description.to_string(),
            subcommand: Some(nested),
            options: None,
            positional_args: None,
            timeout_seconds: None,
            force_synchronous: None,
            enabled,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        }
    }

    #[test]
    fn test_collect_leaf_subcommands_single_default() {
        let subcommands = vec![make_subcommand("default", "Default subcommand", true)];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&subcommands, "", &mut leaves);

        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, "default");
        assert_eq!(leaves[0].1.name, "default");
    }

    #[test]
    fn test_collect_leaf_subcommands_multiple_at_same_level() {
        let subcommands = vec![
            make_subcommand("build", "Build", true),
            make_subcommand("test", "Test", true),
        ];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&subcommands, "", &mut leaves);

        assert_eq!(leaves.len(), 2);
        let names: Vec<&str> = leaves.iter().map(|(path, _)| path.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"test"));
    }

    #[test]
    fn test_collect_leaf_subcommands_nested() {
        let nested = vec![
            make_subcommand("child1", "Child 1", true),
            make_subcommand("child2", "Child 2", true),
        ];

        let subcommands = vec![make_subcommand_with_nested(
            "parent", "Parent", true, nested,
        )];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&subcommands, "", &mut leaves);

        assert_eq!(leaves.len(), 2);
        let paths: Vec<&str> = leaves.iter().map(|(path, _)| path.as_str()).collect();
        assert!(paths.contains(&"parent_child1"));
        assert!(paths.contains(&"parent_child2"));
    }

    #[test]
    fn test_collect_leaf_subcommands_skips_disabled() {
        let subcommands = vec![
            make_subcommand("enabled", "Enabled", true),
            make_subcommand("disabled", "Disabled", false),
        ];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&subcommands, "", &mut leaves);

        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, "enabled");
    }

    #[test]
    fn test_collect_leaf_subcommands_deeply_nested() {
        let level3 = vec![make_subcommand("leaf", "Leaf", true)];
        let level2 = vec![make_subcommand_with_nested("mid", "Mid", true, level3)];
        let level1 = vec![make_subcommand_with_nested("top", "Top", true, level2)];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&level1, "", &mut leaves);

        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, "top_mid_leaf");
    }

    #[test]
    fn test_collect_leaf_subcommands_with_prefix() {
        let subcommands = vec![make_subcommand("child", "Child", true)];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&subcommands, "parent", &mut leaves);

        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, "parent_child");
    }

    #[test]
    fn test_collect_leaf_subcommands_default_uses_prefix() {
        // When subcommand name is "default", it should use just the prefix
        let subcommands = vec![make_subcommand("default", "Default child", true)];

        let mut leaves = Vec::new();
        AhmaMcpService::collect_leaf_subcommands(&subcommands, "parent", &mut leaves);

        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, "parent");
    }

    // ==================== force_synchronous inheritance tests ====================

    #[test]
    fn test_force_synchronous_inheritance_subcommand_overrides_tool() {
        // When subcommand has force_synchronous set, it should override tool level
        let subcommand_sync = Some(true);
        let tool_sync = Some(false);

        // Subcommand wins
        let effective = subcommand_sync.or(tool_sync);
        assert_eq!(effective, Some(true));
    }

    #[test]
    fn test_force_synchronous_inheritance_subcommand_none_inherits_tool() {
        // When subcommand has no force_synchronous, it should inherit from tool
        let subcommand_sync: Option<bool> = None;
        let tool_sync = Some(true);

        // Tool wins when subcommand is None
        let effective = subcommand_sync.or(tool_sync);
        assert_eq!(effective, Some(true));
    }

    #[test]
    fn test_force_synchronous_inheritance_both_none() {
        // When both are None, effective is None (default behavior)
        let subcommand_sync: Option<bool> = None;
        let tool_sync: Option<bool> = None;

        let effective = subcommand_sync.or(tool_sync);
        assert_eq!(effective, None);
    }

    #[test]
    fn test_force_synchronous_subcommand_explicit_false_overrides_tool_true() {
        // Subcommand can explicitly set false to override tool's true
        let subcommand_sync = Some(false);
        let tool_sync = Some(true);

        let effective = subcommand_sync.or(tool_sync);
        assert_eq!(effective, Some(false));
    }
}
