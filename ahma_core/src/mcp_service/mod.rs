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

mod schema;
mod sequence;
mod subcommand;
mod types;

pub use types::{GuidanceConfig, LegacyGuidanceConfig, META_PARAMS, SequenceKind};

use notify::{Event, RecursiveMode, Watcher};
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
use std::path::PathBuf;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};
use tokio::time::Instant;
use tracing;

use crate::{
    adapter::Adapter,
    callback_system::CallbackSender,
    config::{ToolConfig, load_tool_configs},
    mcp_callback::McpCallbackSender,
    operation_monitor::Operation,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// `AhmaMcpService` is the server handler for the MCP service.
#[derive(Clone)]
pub struct AhmaMcpService {
    pub adapter: Arc<Adapter>,
    pub operation_monitor: Arc<crate::operation_monitor::OperationMonitor>,
    pub configs: Arc<RwLock<HashMap<String, ToolConfig>>>,
    pub guidance: Arc<Option<GuidanceConfig>>,
    /// When true, forces all operations to run synchronously (overrides async-by-default).
    /// This is set when the --sync CLI flag is used.
    pub force_synchronous: bool,
    /// When true, sandbox initialization is deferred until roots/list_changed notification.
    /// This is used in HTTP bridge mode where SSE must connect before server→client requests.
    pub defer_sandbox: bool,
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
        force_synchronous: bool,
        defer_sandbox: bool,
    ) -> Result<Self, anyhow::Error> {
        // Start the background monitor for operation timeouts
        crate::operation_monitor::OperationMonitor::start_background_monitor(
            operation_monitor.clone(),
        );

        Ok(Self {
            adapter,
            operation_monitor,
            configs: Arc::new(RwLock::new((*configs).clone())),
            guidance,
            force_synchronous,
            defer_sandbox,
            peer: Arc::new(RwLock::new(None)),
        })
    }

    fn parse_file_uri_to_path(uri: &str) -> Option<PathBuf> {
        const PREFIX: &str = "file://";
        if !uri.starts_with(PREFIX) {
            return None;
        }

        let mut rest = &uri[PREFIX.len()..];

        // Strip any query/fragment.
        if let Some(idx) = rest.find(['?', '#']) {
            rest = &rest[..idx];
        }

        // Accept host form: file://localhost/abs/path
        if let Some(after_localhost) = rest.strip_prefix("localhost") {
            rest = after_localhost;
        }

        // For unix-like paths we accept only absolute paths.
        if !rest.starts_with('/') {
            return None;
        }

        let decoded = Self::percent_decode_utf8(rest)?;
        Some(PathBuf::from(decoded))
    }

    fn percent_decode_utf8(input: &str) -> Option<String> {
        let bytes = input.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;

        while i < bytes.len() {
            match bytes[i] {
                b'%' => {
                    if i + 2 >= bytes.len() {
                        return None;
                    }
                    let hi = bytes[i + 1];
                    let lo = bytes[i + 2];

                    let hex = |b: u8| -> Option<u8> {
                        match b {
                            b'0'..=b'9' => Some(b - b'0'),
                            b'a'..=b'f' => Some(b - b'a' + 10),
                            b'A'..=b'F' => Some(b - b'A' + 10),
                            _ => None,
                        }
                    };

                    let hi = hex(hi)?;
                    let lo = hex(lo)?;
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }

        String::from_utf8(out).ok()
    }

    /// Updates the tool configurations and notifies clients.
    pub async fn update_tools(&self, new_configs: HashMap<String, ToolConfig>) {
        {
            let mut configs_lock = self.configs.write().unwrap();
            *configs_lock = new_configs;
        }

        // Notify clients that the tool list has changed.
        // Clone peer outside the lock before async call to avoid holding guard across .await
        let peer_opt = {
            let peer_lock = self.peer.read().unwrap();
            peer_lock.clone()
        };

        if let Some(peer) = peer_opt {
            if let Err(e) = peer.notify_tool_list_changed().await {
                tracing::error!("Failed to send tools/list_changed notification: {}", e);
            } else {
                tracing::info!("Sent tools/list_changed notification to client");
            }
        } else {
            tracing::debug!("No peer connected, skipping tools/list_changed notification");
        }
    }

    /// Starts a background task to watch for changes in the tools directory.
    pub fn start_config_watcher(&self, tools_dir: PathBuf) {
        let service = self.clone();
        tokio::spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);

            let mut watcher =
                match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        // Only react to relevant events on JSON files or directory changes
                        let relevant = event
                            .paths
                            .iter()
                            .any(|p| p.extension().is_some_and(|ext| ext == "json") || p.is_dir());

                        if relevant
                            && (event.kind.is_modify()
                                || event.kind.is_create()
                                || event.kind.is_remove())
                        {
                            let _ = tx.blocking_send(());
                        }
                    }
                }) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::error!("Failed to create config watcher: {}", e);
                        return;
                    }
                };

            if let Err(e) = watcher.watch(&tools_dir, RecursiveMode::Recursive) {
                tracing::error!("Failed to watch tools directory: {}", e);
                return;
            }

            tracing::info!("Started watching tools directory: {:?}", tools_dir);

            // Debounce logic
            while (rx.recv().await).is_some() {
                // Drain any other events that happened in quick succession
                while rx.try_recv().is_ok() {}

                // Wait a bit for file writes to complete
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                tracing::info!("Detected change in tools directory, reloading configs...");
                match load_tool_configs(&tools_dir).await {
                    Ok(new_configs) => {
                        service.update_tools(new_configs).await;
                        tracing::info!("Successfully reloaded tool configurations");
                    }
                    Err(e) => {
                        tracing::error!("Failed to reload tool configurations: {}", e);
                    }
                }
            }
        });
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

        let input_schema =
            schema::generate_schema_for_tool_config(tool_config, self.guidance.as_ref());

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
                        if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
                            while let Ok(Some(entry)) = entries.next_entry().await {
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
                    if tokio::fs::metadata(".").await.is_ok() {
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
        }
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

    /// Query the client for workspace roots and initialize the sandbox scope.
    ///
    /// This implements the MCP roots protocol where the server requests the
    /// client's workspace roots to establish sandbox boundaries.
    async fn configure_sandbox_from_roots(&self, peer: &Peer<RoleServer>) {
        use crate::sandbox::{get_sandbox_scopes, initialize_sandbox_scopes};

        // Check if sandbox is already configured (e.g., via --sandbox-scope CLI arg)
        if get_sandbox_scopes().is_some() {
            tracing::debug!("Sandbox already configured via CLI, skipping roots/list");
            return;
        }

        tracing::info!("Requesting workspace roots from client via roots/list");

        match peer.list_roots().await {
            Ok(roots_result) => {
                let roots = &roots_result.roots;
                if roots.is_empty() {
                    tracing::warn!(
                        "Client returned empty roots list, sandbox will use fallback behavior"
                    );
                    return;
                }

                // Extract file:// URIs and convert to paths
                let paths: Vec<PathBuf> = roots
                    .iter()
                    .filter_map(|root| Self::parse_file_uri_to_path(&root.uri))
                    .collect();

                if paths.is_empty() {
                    tracing::warn!("No file:// roots found in client response, sandbox unchanged");
                    return;
                }

                tracing::info!(
                    "Received {} workspace root(s) from client: {:?}",
                    paths.len(),
                    paths
                );

                // Initialize sandbox with client's workspace roots
                match initialize_sandbox_scopes(&paths) {
                    Ok(()) => {
                        tracing::info!("Sandbox scope initialized from client roots: {:?}", paths);
                    }
                    Err(e) => {
                        // AlreadyInitialized is expected if CLI arg was used
                        tracing::debug!(
                            "Could not set sandbox from roots (may already be set): {}",
                            e
                        );
                    }
                }
            }
            Err(e) => {
                // Client may not support roots capability - this is not an error
                tracing::info!(
                    "Client does not support roots/list or request failed: {}. \
                     Sandbox will use fallback behavior.",
                    e
                );
            }
        }
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

            // Query client for workspace roots and configure sandbox
            // Per MCP spec, server sends roots/list request to client
            // IMPORTANT: Only do this if sandbox is NOT deferred.
            // In HTTP bridge mode with --defer-sandbox, we wait for roots/list_changed
            // notification which is sent by the bridge when SSE connects.
            if !self.defer_sandbox {
                let peer_clone = peer.clone();
                let service_clone = self.clone();
                tokio::spawn(async move {
                    service_clone
                        .configure_sandbox_from_roots(&peer_clone)
                        .await;
                });
            } else {
                tracing::info!("Sandbox deferred - waiting for roots/list_changed notification");
            }
        }
    }

    fn on_roots_list_changed(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            tracing::info!("Received roots/list_changed notification");

            // This notification is sent by the HTTP bridge when SSE connects.
            // It signals that we can now safely call roots/list.
            let peer = &context.peer;

            // Spawn as background task to avoid blocking message processing
            let peer_clone = peer.clone();
            let service_clone = self.clone();
            tokio::spawn(async move {
                service_clone
                    .configure_sandbox_from_roots(&peer_clone)
                    .await;
            });
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

            {
                let configs_lock = self.configs.read().unwrap();
                for config in configs_lock.values() {
                    if !config.enabled {
                        tracing::debug!(
                            "Skipping disabled tool '{}' during list_tools",
                            config.name
                        );
                        continue;
                    }

                    let tool = self.create_tool_from_config(config);
                    tools.push(tool);
                }
            }

            Ok(ListToolsResult {
                meta: None,
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
            // Acquire read lock for configs, clone the config, and drop the lock immediately
            // to avoid holding the lock across await points (which makes the future !Send)
            let config = {
                let configs_lock = self.configs.read().unwrap();
                match configs_lock.get(tool_name) {
                    Some(config) => config.clone(),
                    None => {
                        let error_message = format!("Tool '{}' not found.", tool_name);
                        tracing::error!("{}", error_message);
                        return Err(McpError::invalid_params(
                            error_message,
                            Some(serde_json::json!({ "tool_name": tool_name })),
                        ));
                    }
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
                return sequence::handle_sequence_tool(
                    &self.adapter,
                    &self.operation_monitor,
                    &self.configs,
                    &config,
                    params,
                    context,
                )
                .await;
            }

            let mut arguments = params.arguments.clone().unwrap_or_default();
            let subcommand_name = arguments
                .remove("subcommand")
                .and_then(|v| v.as_str().map(|s| s.to_string()));

            // Find the subcommand config and construct the command parts
            let (subcommand_config, command_parts) =
                match subcommand::find_subcommand_config_from_args(&config, subcommand_name.clone())
                {
                    Some(result) => result,
                    None => {
                        let has_subcommands = config.subcommand.is_some();
                        let num_subcommands =
                            config.subcommand.as_ref().map(|s| s.len()).unwrap_or(0);
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

            // Delay tool execution until sandbox is initialized from roots/list.
            // This is critical in HTTP bridge mode with deferred sandbox initialization.
            if crate::sandbox::get_sandbox_scopes().is_none() && !crate::sandbox::is_test_mode() {
                let error_message = "Sandbox initializing from client roots - retry tools/call after roots/list completes".to_string();
                tracing::warn!("{}", error_message);
                return Err(McpError::internal_error(error_message, None));
            }

            // Check if the subcommand itself is a sequence
            if subcommand_config.sequence.is_some() {
                return sequence::handle_subcommand_sequence(
                    &self.adapter,
                    &config,
                    subcommand_config,
                    params,
                    context,
                )
                .await;
            }

            let base_command = command_parts.join(" ");

            let working_directory = arguments
                .get("working_directory")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    if crate::sandbox::is_test_mode() {
                        None
                    } else {
                        crate::sandbox::get_sandbox_scope().map(|p| p.to_string_lossy().to_string())
                    }
                })
                .unwrap_or_else(|| ".".to_string());

            let timeout = arguments.get("timeout_seconds").and_then(|v| v.as_u64());

            // Determine execution mode (default is ASYNCHRONOUS):
            // 1. If synchronous=true in config (subcommand or inherited from tool), ALWAYS use sync
            // 2. If synchronous=false in config, ALWAYS use async (explicit async override)
            // 3. If --sync CLI flag was used (force_synchronous=true), use sync mode
            // 4. Check explicit execution_mode argument (for advanced use)
            // 5. Default to ASYNCHRONOUS
            //
            // Inheritance: subcommand.synchronous overrides tool.synchronous
            // If subcommand doesn't specify, inherit from tool level
            let sync_override = subcommand_config.synchronous.or(config.synchronous);
            let execution_mode = if sync_override == Some(true) {
                // Config explicitly requires synchronous: FORCE sync mode
                crate::adapter::ExecutionMode::Synchronous
            } else if sync_override == Some(false) {
                // Config explicitly requires async: FORCE async mode (ignores --sync flag)
                crate::adapter::ExecutionMode::AsyncResultPush
            } else if self.force_synchronous {
                // --sync flag was used and not overridden by config: use sync mode
                crate::adapter::ExecutionMode::Synchronous
            } else if let Some(mode_str) = arguments.get("execution_mode").and_then(|v| v.as_str())
            {
                match mode_str {
                    "Synchronous" => crate::adapter::ExecutionMode::Synchronous,
                    "AsyncResultPush" => crate::adapter::ExecutionMode::AsyncResultPush,
                    _ => crate::adapter::ExecutionMode::AsyncResultPush, // Default to async
                }
            } else {
                // Default to ASYNCHRONOUS mode
                crate::adapter::ExecutionMode::AsyncResultPush
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

#[cfg(test)]
mod tests {
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
