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
        CallToolRequestParam, CallToolResult, Content, ErrorData as McpError, Implementation,
        ListToolsResult, PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo,
        Tool,
    },
    service::{NotificationContext, Peer, RequestContext, RoleServer},
};
use serde_json::Map;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tracing;

use crate::{
    adapter::Adapter,
    config::ToolConfig,
    operation_monitor::{Operation, OperationMonitor},
};

/// `AhmaMcpService` is the server handler for the MCP service.
#[derive(Clone)]
pub struct AhmaMcpService {
    pub adapter: Arc<Adapter>,
    pub operation_monitor: Arc<OperationMonitor>,
    pub configs: Arc<HashMap<String, ToolConfig>>,
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
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            adapter,
            operation_monitor,
            configs,
            peer: Arc::new(RwLock::new(None)),
        })
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

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let mut tools = Vec::new();

            // Hard-wired wait command - always available
            tools.push(Tool {
            name: "wait".into(),
            description: Some("Wait for previously started asynchronous operations to complete. **WARNING:** This is a blocking tool and makes you inefficient. **ONLY** use this if you have NO other tasks and cannot proceed until completion. It is **ALWAYS** better to perform other work and let results be pushed to you.".into()),
            input_schema: Arc::new({
                let mut schema = serde_json::Map::new();
                schema.insert("type".to_string(), "object".into());
                let mut properties = serde_json::Map::new();
                let mut tools_prop = serde_json::Map::new();
                tools_prop.insert("type".to_string(), "string".into());
                tools_prop.insert("description".to_string(), "Comma-separated tool name prefixes to wait for (optional; waits for all if omitted)".into());
                properties.insert("tools".to_string(), tools_prop.into());
                schema.insert("properties".to_string(), properties.into());
                schema
            }),
            output_schema: None,
            annotations: None,
        });

            for config in self.configs.values() {
                if config.subcommand.is_empty() {
                    // If no subcommands defined, register the tool by its base name
                    let input_schema =
                        Arc::new(config.input_schema.as_object().cloned().unwrap_or_default());
                    tools.push(Tool {
                        name: config.name.clone().into(),
                        description: Some(config.description.clone().into()),
                        input_schema,
                        output_schema: None,
                        annotations: None,
                    });
                } else {
                    // Register each subcommand as a separate tool
                    for subcommand in &config.subcommand {
                        let tool_name = format!("{}_{}", config.name, subcommand.name);
                        let input_schema =
                            Arc::new(config.input_schema.as_object().cloned().unwrap_or_default());

                        // Use the full subcommand description, which may include LLM guidance
                        let description = if subcommand.description.is_empty() {
                            format!("{}: {}", config.description, subcommand.name)
                        } else {
                            subcommand.description.clone()
                        };

                        tools.push(Tool {
                            name: tool_name.into(),
                            description: Some(description.into()),
                            input_schema,
                            output_schema: None,
                            annotations: None,
                        });
                    }
                }
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

            if tool_name == "wait" {
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
                    return Ok(CallToolResult::success(vec![Content::text(
                        if tool_filters.is_empty() {
                            "No pending operations to wait for.".to_string()
                        } else {
                            format!(
                                "No pending operations for tools: {}",
                                tool_filters.join(", ")
                            )
                        },
                    )]));
                }

                tracing::info!(
                    "Waiting for {} pending operations: {:?}",
                    pending_ops.len(),
                    pending_ops.iter().map(|op| &op.id).collect::<Vec<_>>()
                );

                // Wait sequentially for all targets
                let mut contents = Vec::new();
                for op in pending_ops {
                    if let Some(done) = self.operation_monitor.wait_for_operation(&op.id).await {
                        match serde_json::to_string_pretty(&done) {
                            Ok(s) => contents.push(Content::text(s)),
                            Err(e) => tracing::error!("Serialization error: {}", e),
                        }
                    }
                }

                return Ok(CallToolResult::success(contents));
            }

            // Parse tool name to extract base command and subcommand
            let (base_command, subcommand) = if let Some(underscore_pos) = tool_name.find('_') {
                let base = &tool_name[..underscore_pos];
                let sub = &tool_name[underscore_pos + 1..];
                (base, Some(sub))
            } else {
                (tool_name, None)
            };

            let config = match self.configs.get(base_command) {
                Some(config) => config,
                None => {
                    let error_message = format!(
                        "Tool '{}' not found (base command '{}' not configured)",
                        tool_name, base_command
                    );
                    tracing::error!("{}", error_message);
                    return Err(McpError::invalid_params(
                        error_message,
                        Some(
                            serde_json::json!({ "tool_name": tool_name, "base_command": base_command }),
                        ),
                    ));
                }
            };

            // Verify subcommand exists if specified and get its config
            let subcommand_config = if let Some(sub) = subcommand {
                match config.subcommand.iter().find(|sc| sc.name == sub) {
                    Some(sc) => Some(sc),
                    None => {
                        let error_message =
                            format!("Subcommand '{}' not found for tool '{}'", sub, base_command);
                        tracing::error!("{}", error_message);
                        return Err(McpError::invalid_params(
                            error_message,
                            Some(
                                serde_json::json!({ "tool_name": tool_name, "base_command": base_command, "subcommand": sub }),
                            ),
                        ));
                    }
                }
            } else {
                None
            };

            // Determine if this operation should be synchronous
            // Async by default, but can be overridden to sync per subcommand
            let is_synchronous = subcommand_config
                .and_then(|sc| {
                    tracing::info!(
                        "Subcommand '{}' synchronous flag: {:?}",
                        sc.name,
                        sc.synchronous
                    );
                    sc.synchronous
                })
                .unwrap_or(false);

            // Determine timeout - subcommand timeout overrides tool timeout
            let timeout = subcommand_config
                .and_then(|sc| sc.timeout_seconds)
                .or(config.timeout_seconds);
            let working_directory = params
                .arguments
                .as_ref()
                .and_then(|args| args.get("working_directory"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            tracing::info!(
                "Executing tool '{}' (base: '{}', subcommand: {:?}) in directory '{}' with mode: {}",
                tool_name,
                base_command,
                subcommand,
                working_directory,
                if is_synchronous {
                    "synchronous"
                } else {
                    "asynchronous"
                }
            );

            let arguments: Option<Map<String, serde_json::Value>> = params.arguments;

            // Modify arguments to include subcommand as first positional argument
            // But only if subcommand is different from the base command to avoid duplication
            let mut modified_args = arguments.unwrap_or_default();
            if let Some(sub) = subcommand {
                // Only add subcommand if it's different from the base command
                // This prevents duplication like "ls ls" when tool is "ls_ls"
                // Also skip generic subcommands like "default" that shouldn't be passed to the command
                if sub != base_command && sub != "default" {
                    modified_args.insert(
                        "_subcommand".to_string(),
                        serde_json::Value::String(sub.to_string()),
                    );
                }
            }

            if is_synchronous {
                match self
                    .adapter
                    .execute_sync_in_dir(
                        &config.command, // Use base command only
                        Some(modified_args),
                        &working_directory,
                        timeout,
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
                // Asynchronous execution (default behavior)
                // Two-stage async pattern: immediate response + background notifications

                // Generate operation ID and create MCP callback for notifications
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
                    .execute_async_in_dir_with_callback_and_id(
                        Some(operation_id.clone()),
                        tool_name,
                        &config.command, // Use base command only
                        Some(modified_args),
                        &working_directory,
                        timeout,
                        callback,
                    )
                    .await;

                tracing::info!(
                    "Asynchronously started tool '{}' with operation ID '{}'",
                    tool_name,
                    operation_id
                );

                // Send immediate "Started" notification using MCP callback
                let peer = context.peer.clone();
                let callback =
                    crate::mcp_callback::mcp_callback(peer.clone(), operation_id.clone());

                // Send started notification asynchronously (don't block the response)
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

                // Store peer handle for later notifications (if not already stored)
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
