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
        ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, Peer, RequestContext, RoleServer},
};
use serde_json::Map;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::time::Instant;
use tracing;

use crate::{
    adapter::Adapter,
    config::{SubcommandConfig, ToolConfig},
    operation_monitor::{Operation, OperationMonitor},
};
use serde_json::Value;

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
    pub operation_monitor: Arc<OperationMonitor>,
    pub configs: Arc<HashMap<String, ToolConfig>>,
    pub guidance: Arc<Option<GuidanceConfig>>,
    /// The peer handle for sending notifications to the client.
    /// This is populated by capturing it from the first request context.
    pub peer: Arc<RwLock<Option<Peer<RoleServer>>>>,
}

impl AhmaMcpService {
    /// Creates a new `AhmaMcpService`.
    pub async fn new(
        adapter: Arc<Adapter>,
        operation_monitor: Arc<OperationMonitor>,
        configs: Arc<HashMap<String, ToolConfig>>,
        guidance: Arc<Option<GuidanceConfig>>,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            adapter,
            operation_monitor,
            configs,
            guidance,
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

    /// Generates the JSON schema for a subcommand's options.
    fn get_schema_for_options(
        &self,
        sub_config: &SubcommandConfig,
    ) -> (Map<String, Value>, Vec<Value>) {
        let mut properties = Map::new();
        let mut required = Vec::new();

        let options = sub_config.options.as_deref().unwrap_or(&[]);
        let positional_args = sub_config.positional_args.as_deref().unwrap_or(&[]);
        let all_options = options.iter().chain(positional_args.iter());

        for option in all_options {
            let mut option_schema = Map::new();
            let param_type = match option.option_type.as_str() {
                "bool" => "boolean",
                "int" => "integer",
                "string" => "string",
                "array" => "array",
                _ => "string",
            };
            option_schema.insert("type".to_string(), Value::String(param_type.to_string()));
            if param_type == "array" {
                let mut items_schema = Map::new();
                items_schema.insert("type".to_string(), Value::String("string".to_string()));
                option_schema.insert("items".to_string(), Value::Object(items_schema));
            }
            option_schema.insert(
                "description".to_string(),
                Value::String(option.description.clone()),
            );
            if let Some(format) = &option.format {
                option_schema.insert("format".to_string(), Value::String(format.clone()));
            }

            properties.insert(option.name.clone(), Value::Object(option_schema));

            if option.required.unwrap_or(false) {
                required.push(Value::String(option.name.clone()));
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
        if let Some(guidance_config) = self.guidance.as_ref() {
            if let Some(guidance_text) = guidance_config.guidance_blocks.get(&tool_name) {
                description = format!("{}\n\n{}", guidance_text, description);
            }
        }

        let input_schema = self.generate_schema_for_tool_config(tool_config);

        Tool {
            name: tool_name.into(),
            description: Some(description.into()),
            input_schema,
            output_schema: None,
            annotations: None,
        }
    }

    /// Finds the configuration for a subcommand from the tool arguments.
    fn find_subcommand_config_from_args<'a>(
        &self,
        tool_config: &'a ToolConfig,
        subcommand_name: Option<String>,
    ) -> Option<(&'a SubcommandConfig, Vec<String>)> {
        let subcommand_path = subcommand_name.unwrap_or_else(|| "default".to_string());
        let subcommand_parts: Vec<&str> = subcommand_path.split('_').collect();

        let mut current_subcommands = tool_config.subcommand.as_ref()?;
        let mut found_subcommand: Option<&SubcommandConfig> = None;
        let mut command_parts = vec![tool_config.command.clone()];

        for (i, part) in subcommand_parts.iter().enumerate() {
            if let Some(sub) = current_subcommands.iter().find(|s| s.name == *part) {
                if sub.name != "default" {
                    command_parts.push(sub.name.clone());
                } else {
                    let config_key = &tool_config.name;
                    let parts: Vec<&str> = config_key.split('_').collect();
                    let derived_subcommand = if parts.len() > 1 {
                        parts[1..].join("-")
                    } else {
                        parts.last().unwrap_or(&"").to_string()
                    };

                    let is_script_like = tool_config.command.contains(' ');
                    let base_command = tool_config
                        .command
                        .split_whitespace()
                        .next()
                        .unwrap_or(&tool_config.command);
                    let should_derive = !derived_subcommand.is_empty()
                        && derived_subcommand != tool_config.command
                        && derived_subcommand != base_command
                        && !is_script_like
                        && config_key.starts_with(base_command)
                        && config_key != base_command;

                    if should_derive {
                        command_parts.push(derived_subcommand);
                    }
                }

                if i == subcommand_parts.len() - 1 {
                    found_subcommand = Some(sub);
                    break;
                }

                if let Some(nested) = &sub.subcommand {
                    current_subcommands = nested;
                } else {
                    return None; // More parts in name, but no more nested subcommands
                }
            } else {
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
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
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

            let active_ops = self.operation_monitor.get_all_operations().await;
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
                description: Some("Wait for previously started asynchronous operations to complete. **WARNING:** This is a blocking tool and makes you inefficient. **ONLY** use this if you have NO other tasks and cannot proceed until completion. It is **ALWAYS** better to perform other work and let results be pushed to you.".into()),
                input_schema: self.generate_input_schema_for_wait(),
                output_schema: None,
                annotations: None,
            });

            // Hard-wired status command - always available
            tools.push(Tool {
                name: "status".into(),
                description: Some("Query the status of operations without blocking. Shows active and completed operations.".into()),
                input_schema: self.generate_input_schema_for_status(),
                output_schema: None,
                annotations: None,
            });

            for config in self.configs.values() {
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
                    .get_all_operations()
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
                        if let Some(end_time) = op.end_time {
                            if let Ok(execution_duration) = end_time.duration_since(op.start_time) {
                                total_execution_time += execution_duration.as_secs_f64();

                                if let Some(first_wait_time) = op.first_wait_time {
                                    if let Ok(wait_duration) =
                                        first_wait_time.duration_since(op.start_time)
                                    {
                                        total_wait_time += wait_duration.as_secs_f64();
                                        operations_with_waits += 1;
                                    }
                                }
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

            let mut arguments = params.arguments.clone().unwrap_or_default();
            let subcommand_name = arguments
                .remove("subcommand")
                .and_then(|v| v.as_str().map(|s| s.to_string()));

            // Find the subcommand config and construct the command parts
            let (subcommand_config, command_parts) =
                match self.find_subcommand_config_from_args(config, subcommand_name) {
                    Some(result) => result,
                    None => {
                        let error_message =
                            format!("Subcommand for tool '{}' not found or invalid.", tool_name);
                        tracing::error!("{}", error_message);
                        return Err(McpError::invalid_params(
                            error_message,
                            Some(serde_json::json!({ "tool_name": tool_name })),
                        ));
                    }
                };

            let base_command = command_parts.join(" ");

            // SECURITY: Path validation
            if let Some(ref args) = params.arguments {
                for (key, value) in args.iter() {
                    if let Some(options) = &subcommand_config.options {
                        if let Some(option_config) = options.iter().find(|o| o.name == *key) {
                            let format = option_config.format.as_deref();
                            if format == Some("path") || format == Some("path-or-value") {
                                if let Some(path_str) = value.as_str() {
                                    if format == Some("path-or-value") && path_str.contains('=') {
                                        continue;
                                    }

                                    let path_to_validate = Path::new(path_str);
                                    let workspace_root = match env::current_dir() {
                                        Ok(dir) => dir,
                                        Err(e) => {
                                            return Err(McpError::internal_error(
                                                format!("Failed to get current directory: {}", e),
                                                None,
                                            ));
                                        }
                                    };

                                    let canonical_workspace = match workspace_root.canonicalize() {
                                        Ok(path) => path,
                                        Err(e) => {
                                            return Err(McpError::internal_error(
                                                format!(
                                                    "Failed to canonicalize workspace path: {}",
                                                    e
                                                ),
                                                None,
                                            ));
                                        }
                                    };

                                    let canonical_path = match path_to_validate.canonicalize() {
                                        Ok(p) => p,
                                        Err(_) => path_to_validate.to_path_buf(),
                                    };

                                    if !canonical_path.starts_with(&canonical_workspace) {
                                        let error_message = format!(
                                            "Path validation failed for parameter '{}'. The path '{}' is outside the allowed workspace.",
                                            key, path_str
                                        );
                                        tracing::error!("{}", error_message);
                                        return Err(McpError::invalid_params(
                                            error_message,
                                            Some(
                                                serde_json::json!({ "parameter": key, "path": path_str }),
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let is_synchronous = subcommand_config
                .synchronous
                .or(config.synchronous)
                .unwrap_or(false);

            let timeout = subcommand_config.timeout_seconds.or(config.timeout_seconds);
            let working_directory = arguments
                .get("working_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            tracing::info!(
                "Executing tool '{}' (command: '{}') in directory '{}' with mode: {}",
                tool_name,
                base_command,
                working_directory,
                if is_synchronous {
                    "synchronous"
                } else {
                    "asynchronous"
                }
            );

            if config.name == "cargo" {
                arguments.remove("working_directory");
            }

            if is_synchronous {
                match self
                    .adapter
                    .execute_sync_in_dir(
                        &base_command,
                        Some(arguments),
                        &working_directory,
                        timeout,
                        Some(subcommand_config),
                    )
                    .await
                {
                    Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
                    Err(e) => {
                        let error_message = format!("Error executing tool '{}': {}", tool_name, e);
                        tracing::error!("{}", error_message);
                        Err(McpError::internal_error(
                            error_message,
                            Some(serde_json::json!({ "details": e.to_string() })),
                        ))
                    }
                }
            } else {
                static OPERATION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
                let op_id = OPERATION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
                let operation_id = format!("op_{}", op_id);

                let peer = context.peer.clone();
                let callback = Some(crate::mcp_callback::mcp_callback(
                    peer.clone(),
                    operation_id.clone(),
                ));

                let _returned_op_id = self
                    .adapter
                    .execute_async_in_dir_with_options(
                        tool_name,
                        &base_command,
                        &working_directory,
                        crate::adapter::AsyncExecOptions {
                            operation_id: Some(operation_id.clone()),
                            args: Some(arguments),
                            timeout,
                            callback,
                            subcommand_config: Some(subcommand_config),
                        },
                    )
                    .await;

                tracing::info!(
                    "Asynchronously started tool '{}' with operation ID '{}'",
                    tool_name,
                    operation_id
                );

                let peer = context.peer.clone();
                let callback =
                    crate::mcp_callback::mcp_callback(peer.clone(), operation_id.clone());

                let operation_id_clone = operation_id.clone();
                let tool_name_clone = tool_name.to_string();
                let working_directory_clone = working_directory.clone();
                tokio::spawn(async move {
                    let started_notification = crate::callback_system::ProgressUpdate::Started {
                        operation_id: operation_id_clone.clone(),
                        command: tool_name_clone.clone(),
                        description: format!(
                            "Executing {} in {}",
                            tool_name_clone, working_directory_clone
                        ),
                    };

                    if let Err(e) = callback.send_progress(started_notification).await {
                        tracing::error!(
                            "Failed to send started notification for operation {}: {:?}",
                            operation_id_clone,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Sent started notification for operation: {}",
                            operation_id_clone
                        );
                    }
                });

                if self.peer.read().unwrap().is_none() {
                    let mut peer_guard = self.peer.write().unwrap();
                    if peer_guard.is_none() {
                        *peer_guard = Some(peer.clone());
                        tracing::info!("Captured MCP peer handle for async notifications");
                    }
                }

                Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string(&serde_json::json!({
                    "status": "started",
                    "job_id": operation_id,
                    "message": format!("Tool '{}' started asynchronously. You will be notified when it is complete.", tool_name)
                }))
                .unwrap(),
            )]))
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
            "timeout_seconds".to_string(),
            serde_json::json!({
                "type": "number",
                "description": "Maximum time to await in seconds (default: 240, min: 10, max: 1800)"
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

    /// Handles the 'await' tool call.
    async fn handle_await(&self, params: CallToolRequestParam) -> Result<CallToolResult, McpError> {
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

        // Parse timeout parameter and implement intelligent timeout calculation
        let (timeout_seconds, timeout_warning) = if let Some(v) = args.get("timeout_seconds") {
            let requested_timeout = v.as_f64().unwrap_or(240.0);
            // Validate timeout: minimum 1s, maximum 1800s (30 minutes)
            let clamped_timeout = if requested_timeout < 1.0 {
                tracing::warn!(
                    "Timeout too small ({}s), using minimum of 1s",
                    requested_timeout
                );
                1.0
            } else if requested_timeout > 1800.0 {
                tracing::warn!(
                    "Timeout too large ({}s), using maximum of 1800s",
                    requested_timeout
                );
                1800.0
            } else {
                requested_timeout
            };

            // Calculate intelligent timeout for comparison
            let intelligent_timeout = self.calculate_intelligent_timeout(&tool_filters).await;

            // Check if requested timeout is less than intelligent timeout
            let warning = if clamped_timeout < intelligent_timeout {
                Some(format!(
                    "⚠️  Timeout of {}s may be insufficient. Operations have max timeout of {}s, suggested minimum: {}s",
                    clamped_timeout as u64, intelligent_timeout as u64, intelligent_timeout as u64
                ))
            } else {
                None
            };

            (clamped_timeout, warning)
        } else {
            // No explicit timeout provided - use intelligent timeout calculation
            let intelligent_timeout = self.calculate_intelligent_timeout(&tool_filters).await;
            (intelligent_timeout, None)
        };

        let timeout_duration = std::time::Duration::from_secs(timeout_seconds as u64);

        // Build from pending ops, optionally filtered by tools
        let pending_ops: Vec<Operation> = self
            .operation_monitor
            .get_all_operations()
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

        // Log timeout warning if one was generated
        if let Some(ref warning) = timeout_warning {
            tracing::warn!("{}", warning);
        }

        let wait_start = Instant::now();
        let (warning_tx, mut warning_rx) = tokio::sync::mpsc::unbounded_channel();

        let warning_task = {
            let warning_tx = warning_tx.clone();
            let timeout_secs = timeout_seconds;
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.5)).await;
                let _ = warning_tx.send(format!(
                    "Wait operation 50% complete ({:.0}s remaining)",
                    timeout_secs * 0.5
                ));
                tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.25)).await;
                let _ = warning_tx.send(format!(
                    "Wait operation 75% complete ({:.0}s remaining)",
                    timeout_secs * 0.25
                ));
                tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.15)).await;
                let _ = warning_tx.send(format!(
                    "Wait operation 90% complete ({:.0}s remaining)",
                    timeout_secs * 0.1
                ));
            })
        };

        let wait_result = tokio::time::timeout(timeout_duration, async {
            let mut contents = Vec::new();
            for op in &pending_ops {
                if let Some(done) = self.operation_monitor.wait_for_operation(&op.id).await {
                    match serde_json::to_string_pretty(&done) {
                        Ok(s) => contents.push(Content::text(s)),
                        Err(e) => tracing::error!("Serialization error: {}", e),
                    }
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

                    // Include timeout warning if present
                    if let Some(warning) = timeout_warning {
                        result_contents.push(Content::text(warning));
                    }

                    result_contents.extend(contents);
                    Ok(CallToolResult::success(result_contents))
                } else {
                    let mut result_contents = vec![Content::text(
                        "No operations completed within timeout period".to_string(),
                    )];

                    // Include timeout warning if present
                    if let Some(warning) = timeout_warning {
                        result_contents.push(Content::text(warning));
                    }

                    Ok(CallToolResult::success(result_contents))
                }
            }
            Err(_) => {
                let elapsed = wait_start.elapsed();
                let still_running: Vec<Operation> = self
                    .operation_monitor
                    .get_all_operations()
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
                    remediation_steps
                        .push("• Try running with offline flags if tool supports them".to_string());
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
                    remediation_steps
                        .push("• Use the 'status' tool to check remaining operations".to_string());
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
    }

    /// Calculate intelligent timeout based on operation timeouts and default await timeout
    ///
    /// Returns the maximum of:
    /// 1. Default await timeout (240 seconds)
    /// 2. Maximum timeout of all pending operations (filtered by tool if specified)
    pub async fn calculate_intelligent_timeout(&self, tool_filters: &[String]) -> f64 {
        const DEFAULT_AWAIT_TIMEOUT: f64 = 240.0; // 4 minutes
        const DEFAULT_OPERATION_TIMEOUT: f64 = 300.0; // 5 minutes - fallback for operations without explicit timeout

        // Get all pending operations, filtered by tools if specified
        let pending_ops: Vec<crate::operation_monitor::Operation> = self
            .operation_monitor
            .get_all_operations()
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
            return DEFAULT_AWAIT_TIMEOUT;
        }

        // Find the maximum timeout among pending operations
        let max_operation_timeout = pending_ops
            .iter()
            .map(|op| {
                op.timeout_duration
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(DEFAULT_OPERATION_TIMEOUT)
            })
            .fold(0.0, f64::max);

        // Return the maximum of default await timeout and max operation timeout
        DEFAULT_AWAIT_TIMEOUT.max(max_operation_timeout)
    }
}
