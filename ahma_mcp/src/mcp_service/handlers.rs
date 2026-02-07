use crate::mcp_callback::McpCallbackSender;
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, Content, ErrorData as McpError},
    service::{RequestContext, RoleServer},
};
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::time::Instant;
use tracing;

use super::{AhmaMcpService, NEXT_ID};
use crate::{
    callback_system::CallbackSender, client_type::McpClientType, operation_monitor::Operation,
};
use std::sync::atomic::Ordering;

impl AhmaMcpService {
    /// Generates the specific input schema for the `await` tool.
    pub fn generate_input_schema_for_wait(&self) -> Arc<Map<String, Value>> {
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
    pub fn generate_input_schema_for_status(&self) -> Arc<Map<String, Value>> {
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

    /// Generates the specific input schema for the `sandboxed_shell` tool.
    pub fn generate_input_schema_for_sandboxed_shell(&self) -> Arc<Map<String, Value>> {
        let mut properties = Map::new();
        properties.insert(
            "command".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "The shell command to execute (supports pipes, redirects, variables, etc.)"
            }),
        );
        properties.insert(
            "working_directory".to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Working directory for command execution",
                "format": "path"
            }),
        );

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(properties));
        schema.insert(
            "required".to_string(),
            Value::Array(vec![Value::String("command".to_string())]),
        );
        Arc::new(schema)
    }

    /// Handles the 'await' tool call.
    pub async fn handle_await(
        &self,
        params: CallToolRequestParams,
    ) -> Result<CallToolResult, McpError> {
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

    /// Handles the 'status' tool call.
    pub async fn handle_status(
        &self,
        args: Map<String, Value>,
    ) -> Result<CallToolResult, McpError> {
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
        let specific_operation_id: Option<String> = if let Some(v) = args.get("operation_id") {
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
                        && let Ok(wait_duration) = first_wait_time.duration_since(op.start_time)
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

        Ok(CallToolResult::success(contents))
    }

    /// Handles the 'cancel' tool call.
    pub async fn handle_cancel(
        &self,
        args: Map<String, Value>,
    ) -> Result<CallToolResult, McpError> {
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
            if let Some(operation) = self.operation_monitor.get_operation(&operation_id).await {
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

        Ok(CallToolResult::success(vec![
            Content::text(result_message),
            Content::text(suggestion.to_string()),
        ]))
    }

    /// Handles the 'sandboxed_shell' built-in tool call.
    /// Executes shell commands using bash within the sandbox.
    /// Supports both synchronous and asynchronous execution modes.
    pub async fn handle_sandboxed_shell(
        &self,
        params: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = params.arguments.unwrap_or_default();

        // Extract command (required)
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpError::invalid_params("command parameter is required".to_string(), None)
            })?
            .to_string();

        // Extract working_directory (optional)
        let working_directory = args
            .get("working_directory")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if self.adapter.sandbox().is_test_mode() {
                    None
                } else {
                    self.adapter
                        .sandbox()
                        .scopes()
                        .first()
                        .map(|p: &std::path::PathBuf| p.to_string_lossy().to_string())
                }
            })
            .unwrap_or_else(|| ".".to_string());

        let timeout = args.get("timeout_seconds").and_then(|v| v.as_u64());

        // Determine execution mode
        // 1. If --sync CLI flag was used (force_synchronous=true), use sync mode
        // 2. Check explicit execution_mode argument
        // 3. Default to ASYNCHRONOUS (like other tools)
        let execution_mode = if self.force_synchronous {
            crate::adapter::ExecutionMode::Synchronous
        } else if let Some(mode_str) = args.get("execution_mode").and_then(|v| v.as_str()) {
            match mode_str {
                "Synchronous" => crate::adapter::ExecutionMode::Synchronous,
                "AsyncResultPush" => crate::adapter::ExecutionMode::AsyncResultPush,
                _ => crate::adapter::ExecutionMode::AsyncResultPush,
            }
        } else {
            crate::adapter::ExecutionMode::AsyncResultPush
        };

        // Build arguments map for adapter
        let mut adapter_args = Map::new();
        adapter_args.insert("command".to_string(), serde_json::Value::String(command));
        if let Some(wd) = args.get("working_directory") {
            adapter_args.insert("working_directory".to_string(), wd.clone());
        }

        // Create a minimal SubcommandConfig for bash -c
        let subcommand_config = crate::config::SubcommandConfig {
            name: "sandboxed_shell".to_string(),
            description: "Execute shell commands".to_string(),
            subcommand: None,
            options: Some(vec![crate::config::CommandOption {
                name: "c_flag".to_string(), // Internal name
                option_type: "boolean".to_string(),
                description: Some("Execute command string".to_string()),
                required: Some(false),
                format: None,
                items: None,
                file_arg: None,
                file_flag: None,
                alias: Some("c".to_string()), // Single-char alias will create -c
            }]),
            positional_args: Some(vec![crate::config::CommandOption {
                name: "command".to_string(),
                option_type: "string".to_string(),
                description: Some("Shell command to execute".to_string()),
                required: Some(true),
                format: None,
                items: None,
                file_arg: None,
                file_flag: None,
                alias: None,
            }]),
            positional_args_first: Some(false),
            timeout_seconds: timeout,
            synchronous: Some(matches!(
                execution_mode,
                crate::adapter::ExecutionMode::Synchronous
            )),
            enabled: true,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        };

        // Add the -c flag to adapter_args using the internal name
        adapter_args.insert("c_flag".to_string(), serde_json::Value::Bool(true));

        match execution_mode {
            crate::adapter::ExecutionMode::Synchronous => {
                let operation_id = format!("op_{}", NEXT_ID.fetch_add(1, Ordering::SeqCst));
                let progress_token = context.meta.get_progress_token();
                let client_type = McpClientType::from_peer(&context.peer);

                // Send 'Started' notification if progress token is present
                if let Some(token) = progress_token.clone() {
                    let callback = McpCallbackSender::new(
                        context.peer.clone(),
                        operation_id.clone(),
                        Some(token),
                        client_type,
                    );
                    let _ = callback
                        .send_progress(crate::callback_system::ProgressUpdate::Started {
                            operation_id: operation_id.clone(),
                            command: "/bin/bash".to_string(),
                            description: format!("Execute /bin/bash in {}", working_directory),
                        })
                        .await;
                }

                let result = self
                    .adapter
                    .execute_sync_in_dir(
                        "/bin/bash",
                        Some(adapter_args),
                        &working_directory,
                        timeout,
                        Some(&subcommand_config),
                    )
                    .await;

                // Send completion notification if progress token is present
                if let Some(token) = progress_token {
                    let callback = McpCallbackSender::new(
                        context.peer.clone(),
                        operation_id.clone(),
                        Some(token),
                        client_type,
                    );
                    match &result {
                        Ok(output) => {
                            let _ = callback
                                .send_progress(
                                    crate::callback_system::ProgressUpdate::FinalResult {
                                        operation_id: operation_id.clone(),
                                        command: "/bin/bash".to_string(),
                                        description: format!(
                                            "Execute /bin/bash in {}",
                                            working_directory
                                        ),
                                        working_directory: working_directory.clone(),
                                        success: true,
                                        duration_ms: 0, // Duration not easily available here
                                        full_output: output.clone(),
                                    },
                                )
                                .await;
                        }
                        Err(e) => {
                            let _ = callback
                                .send_progress(
                                    crate::callback_system::ProgressUpdate::FinalResult {
                                        operation_id: operation_id.clone(),
                                        command: "/bin/bash".to_string(),
                                        description: format!(
                                            "Execute /bin/bash in {}",
                                            working_directory
                                        ),
                                        working_directory: working_directory.clone(),
                                        success: false,
                                        duration_ms: 0,
                                        full_output: format!("Error: {}", e),
                                    },
                                )
                                .await;
                        }
                    }
                }

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
                let progress_token = context.meta.get_progress_token();
                let client_type = McpClientType::from_peer(&context.peer);
                let callback: Option<Box<dyn CallbackSender>> = progress_token.map(|token| {
                    Box::new(McpCallbackSender::new(
                        context.peer.clone(),
                        operation_id.clone(),
                        Some(token),
                        client_type,
                    )) as Box<dyn CallbackSender>
                });

                let job_id = self
                    .adapter
                    .execute_async_in_dir_with_options(
                        "sandboxed_shell",
                        "/bin/bash",
                        &working_directory,
                        crate::adapter::AsyncExecOptions {
                            operation_id: Some(operation_id),
                            args: Some(adapter_args),
                            timeout,
                            callback,
                            subcommand_config: Some(&subcommand_config),
                        },
                    )
                    .await;

                match job_id {
                    Ok(id) => {
                        let hint = crate::tool_hints::preview(&id, "sandboxed_shell");
                        let message =
                            format!("Asynchronous operation started with ID: {}{}", id, hint);
                        Ok(CallToolResult::success(vec![Content::text(message)]))
                    }
                    Err(e) => {
                        let error_message = format!("Async execution failed: {}", e);
                        tracing::error!("{}", error_message);
                        Err(McpError::internal_error(error_message, None))
                    }
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
}
