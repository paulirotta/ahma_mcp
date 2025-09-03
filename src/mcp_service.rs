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
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
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
                if config.subcommand.is_empty() {
                    // This case might be for tools without subcommands.
                    // We'll assume for now that all tools have subcommands,
                    // as per the current design.
                } else {
                    // Register each subcommand as a separate tool
                    for subcommand in &config.subcommand {
                        let tool_name = format!("{}_{}", config.name, subcommand.name);
                        let input_schema =
                            self.generate_input_schema_for_subcommand(config, subcommand);

                        let mut description = subcommand.description.clone();
                        if let Some(guidance_key) = &subcommand.guidance_key {
                            if let Some(guidance_config) = self.guidance.as_ref() {
                                // First try the new guidance_blocks structure
                                if let Some(guidance_text) =
                                    guidance_config.guidance_blocks.get(guidance_key)
                                {
                                    description = format!("{}\n\n{}", guidance_text, description);
                                }
                                // Fallback to legacy structure for backward compatibility
                                else if let Some(legacy_config) = &guidance_config.legacy_guidance
                                {
                                    if let Some(guidance_text) = legacy_config
                                        .tool_specific_guidance
                                        .get(&config.name)
                                        .and_then(|g| g.get(guidance_key))
                                    {
                                        description =
                                            format!("{}\n\n{}", guidance_text, description);
                                    }
                                }
                            }
                        }

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

                // ============================================================================
                // CRITICAL: Wait Tool Timeout Implementation
                // ============================================================================
                //
                // PURPOSE: Implements "I think 'wait' should have an optional timeout,
                //          and a default timeout of 240sec"
                //
                // LESSON LEARNED: Default changed from 300s to 240s per user request.
                // Validation bounds prevent user errors and resource waste:
                // - Minimum 10s: Prevents accidentally short timeouts
                // - Maximum 1800s (30min): Prevents runaway waits
                // - Default 240s (4min): Balance of patience vs efficiency
                //
                // DO NOT CHANGE: These values were established through user testing
                // ============================================================================

                // Parse timeout parameter (default 240 seconds = 4 minutes, with validation)
                let timeout_seconds = if let Some(v) = args.get("timeout_seconds") {
                    let requested_timeout = v.as_f64().unwrap_or(240.0);
                    // Validate timeout: minimum 10s, maximum 1800s (30 minutes)
                    if requested_timeout < 10.0 {
                        tracing::warn!(
                            "Timeout too small ({}s), using minimum of 10s",
                            requested_timeout
                        );
                        10.0
                    } else if requested_timeout > 1800.0 {
                        tracing::warn!(
                            "Timeout too large ({}s), using maximum of 1800s",
                            requested_timeout
                        );
                        1800.0
                    } else {
                        requested_timeout
                    }
                } else {
                    240.0 // CRITICAL: Changed from 300.0 to 240.0 per user requirement
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
                    // Before declaring no pending ops, check the history for recently completed ones
                    // that match the filter. This avoids a confusing state where a user waits
                    // for a tool that just finished.
                    let completed_ops = self.operation_monitor.get_completed_operations().await;
                    let relevant_completed: Vec<Operation> = completed_ops
                        .into_iter()
                        .filter(|op| {
                            if tool_filters.is_empty() {
                                false // Only show completed if a filter is active
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
                    "Waiting for {} pending operations (timeout: {}s): {:?}",
                    pending_ops.len(),
                    timeout_seconds,
                    pending_ops.iter().map(|op| &op.id).collect::<Vec<_>>()
                );

                // Apply global timeout to the entire wait operation with progressive warnings
                let wait_start = std::time::Instant::now();

                // Create a channel for timeout warnings
                let (warning_tx, mut warning_rx) = tokio::sync::mpsc::unbounded_channel();

                // Spawn a task to send progressive timeout warnings
                let warning_task = {
                    let warning_tx = warning_tx.clone();
                    let timeout_secs = timeout_seconds;
                    tokio::spawn(async move {
                        // 50% warning
                        tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.5))
                            .await;
                        let _ = warning_tx.send(format!(
                            "Wait operation 50% complete ({:.0}s remaining)",
                            timeout_secs * 0.5
                        ));

                        // 75% warning
                        tokio::time::sleep(std::time::Duration::from_secs_f64(timeout_secs * 0.25))
                            .await;
                        let _ = warning_tx.send(format!(
                            "Wait operation 75% complete ({:.0}s remaining)",
                            timeout_secs * 0.25
                        ));

                        // 90% warning
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
                    for op in &pending_ops {
                        if let Some(done) = self.operation_monitor.wait_for_operation(&op.id).await
                        {
                            match serde_json::to_string_pretty(&done) {
                                Ok(s) => contents.push(Content::text(s)),
                                Err(e) => tracing::error!("Serialization error: {}", e),
                            }
                        }
                    }
                    contents
                })
                .await;

                // Cancel the warning task
                warning_task.abort();

                // Drain any remaining warnings
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
                            return Ok(CallToolResult::success(result_contents));
                        } else {
                            return Ok(CallToolResult::success(vec![Content::text(
                                "No operations completed within timeout period".to_string(),
                            )]));
                        }
                    }
                    Err(_) => {
                        let elapsed = wait_start.elapsed();

                        // Check what operations are still running
                        let still_running: Vec<Operation> = self
                            .operation_monitor
                            .get_all_operations()
                            .await
                            .into_iter()
                            .filter(|op| !op.state.is_terminal())
                            .collect();

                        let completed_during_wait = pending_ops.len() - still_running.len();

                        // Check for common issues that cause timeouts
                        let mut remediation_steps = Vec::new();

                        // Generic lock file detection - check for various lock patterns
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

                        // Check available disk space
                        if std::fs::metadata(".").is_ok() {
                            // This is a simple check - in a real implementation we'd use a crate like `fs2` for proper disk space checking
                            // For now, just suggest checking disk space
                            remediation_steps
                                .push("• Check available disk space: df -h .".to_string());
                        }

                        // Check for process conflicts - look for running instances of the same commands
                        let running_commands: std::collections::HashSet<String> = still_running
                            .iter()
                            .map(|op| {
                                // Extract base command from tool name (e.g., "cargo_build" -> "cargo")
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

                        // Check for network-related operations that might be slow
                        let network_keywords = [
                            "audit", "update", "search", "add", "install", "fetch", "clone",
                            "pull", "push", "download", "upload", "sync",
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
                                "• Try running with offline flags if tool supports them"
                                    .to_string(),
                            );
                        }

                        // Check for potentially long-running build/compile operations
                        let build_keywords = [
                            "build", "compile", "test", "lint", "clippy", "format", "check",
                            "verify", "validate", "analyze",
                        ];
                        let has_build_ops = still_running.iter().any(|op| {
                            build_keywords
                                .iter()
                                .any(|keyword| op.tool_name.contains(keyword))
                        });

                        if has_build_ops {
                            remediation_steps.push("• Build/compile operations can take time - consider increasing timeout_seconds".to_string());
                            remediation_steps
                                .push("• Check system resources: top or htop".to_string());
                            remediation_steps.push(
                                "• Consider running operations with verbose flags to see progress"
                                    .to_string(),
                            );
                        }

                        if remediation_steps.is_empty() {
                            remediation_steps.push(
                                "• Use the 'status' tool to check remaining operations".to_string(),
                            );
                            remediation_steps.push("• Operations continue running in background - they may complete shortly".to_string());
                            remediation_steps.push("• Consider increasing timeout_seconds if operations legitimately need more time".to_string());
                        }

                        let mut error_message = format!(
                            "Wait operation timed out after {:.2}s (configured timeout: {:.0}s).\n\n\
                            Progress: {}/{} operations completed during wait.\n\
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
                                error_message
                                    .push_str(&format!("\n• {} ({})", op.id, op.tool_name));
                            }
                        }

                        return Ok(CallToolResult::success(vec![Content::text(error_message)]));
                    }
                }
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

            // SECURITY: Path validation
            // Before executing, validate any arguments that are designated as paths.
            if let Some(sub_config) = subcommand_config {
                if let Some(ref args) = params.arguments {
                    for (key, value) in args.iter() {
                        // Check if the argument is a path that needs validation
                        if let Some(option_config) =
                            sub_config.options.iter().find(|o| o.name == *key)
                        {
                            let format = option_config.format.as_deref();
                            if format == Some("path") || format == Some("path-or-value") {
                                if let Some(path_str) = value.as_str() {
                                    // For "path-or-value", skip validation if it's a KEY=VALUE string
                                    if format == Some("path-or-value") && path_str.contains('=') {
                                        continue;
                                    }

                                    let path_to_validate = Path::new(path_str);

                                    // Get the workspace root from the current working directory
                                    let workspace_root = match env::current_dir() {
                                        Ok(dir) => dir,
                                        Err(e) => {
                                            return Err(McpError::internal_error(
                                                format!("Failed to get current directory: {}", e),
                                                None,
                                            ));
                                        }
                                    };

                                    // Canonicalize both paths to resolve any '..' or symlinks
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
                                        Err(_) => {
                                            // If canonicalization fails, it might be because the path doesn't exist yet.
                                            // In that case, we check the absolute path.
                                            path_to_validate.to_path_buf()
                                        }
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

            // Determine if this operation should be synchronous
            // 1. If subcommand.synchronous is Some(value), use value
            // 2. If subcommand.synchronous is None, inherit tool.synchronous
            // 3. If tool.synchronous is None, default to false (async)
            let is_synchronous = subcommand_config
                .and_then(|sc| {
                    tracing::info!(
                        "Subcommand '{}' synchronous flag: {:?}",
                        sc.name,
                        sc.synchronous
                    );
                    sc.synchronous
                })
                .or(config.synchronous)
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

            // SECURITY FIX: Filter out working_directory for cargo commands to prevent injection attacks
            // Cargo should always run in the current working directory where the server started
            if base_command == "cargo" {
                modified_args.remove("working_directory");
            }

            if is_synchronous {
                match self
                    .adapter
                    .execute_sync_in_dir(
                        &config.command, // Use base command only
                        Some(modified_args),
                        &working_directory,
                        timeout,
                        subcommand_config,
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
                    .execute_async_in_dir_with_options(
                        tool_name,
                        &config.command, // Use base command only
                        &working_directory,
                        crate::adapter::AsyncExecOptions {
                            operation_id: Some(operation_id.clone()),
                            args: Some(modified_args),
                            timeout,
                            callback,
                            subcommand_config,
                        },
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

impl AhmaMcpService {
    /// Generates a JSON schema for a given subcommand's inputs.
    fn generate_input_schema_for_subcommand(
        &self,
        tool_config: &ToolConfig,
        subcommand_config: &SubcommandConfig,
    ) -> Arc<Map<String, Value>> {
        let mut properties = Map::new();
        let mut required = Vec::new();

        // Add common `working_directory` parameter unless it's a cargo command
        if tool_config.name != "cargo" {
            let mut wd_schema = Map::new();
            wd_schema.insert("type".to_string(), Value::String("string".to_string()));
            wd_schema.insert(
                "description".to_string(),
                Value::String("Working directory for command execution".to_string()),
            );
            wd_schema.insert("format".to_string(), Value::String("path".to_string()));
            properties.insert("working_directory".to_string(), Value::Object(wd_schema));
        }

        let all_options = subcommand_config
            .options
            .iter()
            .chain(subcommand_config.positional_args.iter());

        for option in all_options {
            let mut option_schema = Map::new();
            let param_type = match option.option_type.as_str() {
                "bool" => "boolean",
                "int" => "integer",
                "string" => "string",
                "array" => "array",
                _ => "string", // Default to string for safety
            };
            option_schema.insert("type".to_string(), Value::String(param_type.to_string()));

            // CRITICAL FIX: For array types, add required "items" property
            // This prevents catastrophic MCP validation failures in VSCode GitHub Copilot Chat
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

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(properties));
        if !required.is_empty() {
            schema.insert("required".to_string(), Value::Array(required));
        }

        Arc::new(schema)
    }

    /// Generates the specific input schema for the `wait` tool.
    fn generate_input_schema_for_wait(&self) -> Arc<Map<String, Value>> {
        let mut properties = Map::new();
        properties.insert(
            "tools".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Comma-separated tool name prefixes to wait for (optional; waits for all if omitted)"
            }),
        );
        properties.insert(
            "timeout_seconds".to_string(),
            serde_json::json!({
                "type": "number",
                "description": "Maximum time to wait in seconds (default: 240, min: 10, max: 1800)"
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
}
