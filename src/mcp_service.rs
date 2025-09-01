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
use std::sync::{Arc, RwLock};
use tracing::{error, info};

use crate::{
    adapter::{Adapter, ExecutionMode},
    config::ToolConfig,
    operation_monitor::OperationMonitor,
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
impl ServerHandler for AhmaMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: None,
        }
    }

    fn on_initialized(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            info!("Client connected: {context:?}");
            // Get the peer from the context
            let peer = &context.peer;
            if self.peer.read().unwrap().is_none() {
                let mut peer_guard = self.peer.write().unwrap();
                if peer_guard.is_none() {
                    *peer_guard = Some(peer.clone());
                    info!("Successfully captured MCP peer handle for async notifications.");
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
            let tools = self
                .configs
                .values()
                .map(|config| {
                    let input_schema =
                        Arc::new(config.input_schema.as_object().cloned().unwrap_or_default());
                    Tool {
                        name: config.name.clone().into(),
                        description: Some(config.description.clone().into()),
                        input_schema,
                        output_schema: None,
                        annotations: None,
                    }
                })
                .collect();
            Ok(ListToolsResult {
                tools,
                next_cursor: None,
            })
        }
    }

    fn call_tool(
        &self,
        params: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let tool_name = params.name.as_ref();
            let config = match self.configs.get(tool_name) {
                Some(config) => config,
                None => {
                    let error_message = format!("Tool '{}' not found", tool_name);
                    error!("{}", error_message);
                    return Err(McpError::invalid_params(
                        error_message,
                        Some(serde_json::json!({ "tool_name": tool_name })),
                    ));
                }
            };

            let execution_mode = config.execution_mode;
            let working_directory = params
                .arguments
                .as_ref()
                .and_then(|args| args.get("working_directory"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            info!(
                "Executing tool '{}' in directory '{}' with mode: {:?}",
                tool_name, working_directory, execution_mode
            );

            let arguments: Option<Map<String, serde_json::Value>> = params.arguments;

            match execution_mode {
                ExecutionMode::Synchronous => {
                    match self
                        .adapter
                        .execute_sync_in_dir(
                            &config.command,
                            arguments,
                            &working_directory,
                            config.timeout,
                        )
                        .await
                    {
                        Ok(output) => Ok(CallToolResult::success(vec![Content::text(output)])),
                        Err(e) => {
                            let error_message =
                                format!("Error executing tool '{}': {}", tool_name, e);
                            error!("{}", error_message);
                            Err(McpError::internal_error(
                                error_message,
                                Some(serde_json::json!({ "details": e.to_string() })),
                            ))
                        }
                    }
                }
                ExecutionMode::AsyncResultPush => {
                    let operation_id = self
                        .adapter
                        .execute_async_in_dir(
                            &config.command,
                            arguments,
                            &working_directory,
                            config.timeout,
                        )
                        .await;

                    info!(
                        "Asynchronously started tool '{}' with operation ID '{}'",
                        tool_name, operation_id
                    );

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
}
