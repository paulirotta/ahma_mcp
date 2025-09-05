//! # CLI Tool Adapter
//!
//! This module provides the core execution engine for running external command-line tools.
//! Its primary responsibility is to execute commands, either synchronously or asynchronously,
//! and manage a pool of reusable shell processes for performance.
//!
//! ## Core Components
//!
//! - **`Adapter`**: The central struct that manages command execution. It holds the
//!   `ShellPoolManager` for efficient asynchronous command execution and configuration
//!   for timeout and synchronous mode.
//!
//! ## Execution Flow
//!
//! 1. **Initialization**:
//!    - The `Adapter` is created with settings for synchronous mode and command timeouts.
//!    - It initializes a `ShellPoolManager` to handle a pool of warm shell processes,
//!      which reduces the overhead of spawning new processes for each command.
//!
//! 2. **Execution (`execute_tool_in_dir`)**:
//!    - A tool execution request is received with a base command, arguments, and a working directory.
//!    - The `Adapter` determines whether to run the command synchronously or asynchronously based
//!      on its `synchronous_mode` setting.
//!    - **Async Path**: If not in synchronous mode and a working directory is provided, a
//!      pre-warmed shell is requested from the `ShellPoolManager`. The command is sent to the
//!      shell for execution, and the shell is returned to the pool afterward. This is the
//!      preferred path for performance.
//!    - **Sync Path**: If in synchronous mode, or if no working directory is given (making the
//!      shell pool less effective), the command is executed as a standard `tokio::process::Command`.
//!
//! ## Key Design Decisions
//!
//! - **Stateless Execution**: The `Adapter` is stateless regarding tool definitions. It simply
//!   receives a command and arguments and executes them. All logic for tool discovery and
//!   argument parsing is handled by the `mcp_service` and `main` modules.
//! - **Performance-Oriented Async**: The integration with `ShellPoolManager` ensures that
//!   asynchronous operations are executed with minimal overhead, which is critical for a
//!   responsive user experience in a server context.

use crate::{
    operation_monitor::{Operation, OperationMonitor, OperationStatus},
    shell_pool::{ShellCommand, ShellPoolManager},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

static OPERATION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_operation_id() -> String {
    let id = OPERATION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("op_{}", id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    Synchronous,
    AsyncResultPush,
}

/// Options for configuring asynchronous execution.
pub struct AsyncExecOptions<'a> {
    /// Optional pre-defined operation ID to use; if None, a new one is generated.
    pub operation_id: Option<String>,
    /// Structured arguments for the command (positional and flags derived internally).
    pub args: Option<Map<String, serde_json::Value>>,
    /// Timeout in seconds for the command; falls back to shell pool default if None.
    pub timeout: Option<u64>,
    /// Optional callback to receive progress and final result notifications.
    pub callback: Option<Box<dyn crate::callback_system::CallbackSender>>,
    /// Subcommand configuration for handling positional arguments and aliases.
    pub subcommand_config: Option<&'a crate::config::SubcommandConfig>,
}

/// The main adapter that handles command execution.
#[derive(Debug)]
pub struct Adapter {
    /// Operation monitor for async tasks.
    monitor: Arc<OperationMonitor>,
    /// Pre-warmed shell pool manager for async execution.
    shell_pool: Arc<ShellPoolManager>,
    /// Handles to spawned tasks for graceful shutdown.
    task_handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// Temporary files created for multi-line arguments - cleaned up automatically when dropped
    temp_files: Arc<Mutex<Vec<NamedTempFile>>>,
}

impl Adapter {
    /// Create a new adapter with a specific timeout.
    pub fn new(monitor: Arc<OperationMonitor>, shell_pool: Arc<ShellPoolManager>) -> Result<Self> {
        Ok(Self {
            monitor,
            shell_pool,
            task_handles: Arc::new(Mutex::new(HashMap::new())),
            temp_files: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Gracefully shuts down the adapter by waiting for all spawned tasks to complete.
    pub async fn shutdown(&self) {
        let mut handles = self.task_handles.lock().await;
        for (id, handle) in handles.drain() {
            tracing::debug!("Waiting for task {} to complete...", id);
            if let Err(e) = handle.await {
                tracing::error!("Error waiting for task {}: {:?}", id, e);
            }
        }
    }

    /// Synchronously executes a command and returns the result directly.
    pub async fn execute_sync_in_dir(
        &self,
        command: &str,
        args: Option<Map<String, serde_json::Value>>,
        working_dir: &str,
        timeout_seconds: Option<u64>,
        subcommand_config: Option<&crate::config::SubcommandConfig>,
    ) -> Result<String, anyhow::Error> {
        let mut shell = self
            .shell_pool
            .get_shell(working_dir)
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to get shell from pool"))?;

        let (command_with_subcommand, args_vec) = self
            .prepare_command_and_args(command, args.as_ref(), subcommand_config)
            .await?;

        let shell_cmd = ShellCommand {
            id: generate_operation_id(),
            command: [command_with_subcommand]
                .iter()
                .map(ToString::to_string)
                .chain(args_vec.into_iter())
                .collect(),
            working_dir: working_dir.to_string(),
            timeout_ms: timeout_seconds
                .unwrap_or_else(|| self.shell_pool.config().command_timeout.as_millis() as u64)
                * 1000,
        };

        let result = shell.execute_command(shell_cmd).await;
        self.shell_pool.return_shell(shell).await;

        match result {
            Ok(output) => {
                if output.exit_code == 0 {
                    // For synchronous tools, return both stdout and stderr for completeness
                    // Many tools (like cargo) write informational output to stderr
                    let result_content = if output.stdout.is_empty() && !output.stderr.is_empty() {
                        // If stdout is empty but stderr has content, return stderr (e.g., cargo check)
                        output.stderr
                    } else if !output.stdout.is_empty() && !output.stderr.is_empty() {
                        // If both have content, combine them
                        format!("{}\n{}", output.stdout, output.stderr)
                    } else {
                        // Otherwise, return stdout (which could be empty)
                        output.stdout
                    };
                    Ok(result_content)
                } else {
                    Err(anyhow::anyhow!(
                        "Command failed with exit code {}: stderr: {}, stdout: {}",
                        output.exit_code,
                        output.stderr,
                        output.stdout
                    ))
                }
            }
            Err(e) => {
                let error_message = e.to_string();

                // TIMEOUT FIX: Detect shell timeout errors and provide descriptive message
                if error_message.contains("Shell communication timeout")
                    || error_message.contains("timeout")
                    || error_message.contains("Timeout")
                {
                    Err(anyhow::anyhow!(
                        "Operation timed out (exceeded timeout limit): {}",
                        error_message
                    ))
                } else {
                    Err(anyhow::anyhow!("Command execution failed: {}", e))
                }
            }
        }
    }

    /// Asynchronously starts a command, returns a job_id, and pushes the result later.
    pub async fn execute_async_in_dir(
        &self,
        tool_name: &str,
        command: &str,
        args: Option<Map<String, serde_json::Value>>,
        working_directory: &str,
        timeout: Option<u64>,
    ) -> Result<String> {
        self.execute_async_in_dir_with_options(
            tool_name,
            command,
            working_directory,
            AsyncExecOptions {
                operation_id: None,
                args,
                timeout,
                callback: None,
                subcommand_config: None,
            },
        )
        .await
    }

    /// Asynchronously starts a command with callback support for notifications
    pub async fn execute_async_in_dir_with_callback(
        &self,
        tool_name: &str,
        command: &str,
        args: Option<Map<String, serde_json::Value>>,
        working_directory: &str,
        timeout: Option<u64>,
        callback: Option<Box<dyn crate::callback_system::CallbackSender>>,
    ) -> Result<String> {
        self.execute_async_in_dir_with_options(
            tool_name,
            command,
            working_directory,
            AsyncExecOptions {
                operation_id: None,
                args,
                timeout,
                callback,
                subcommand_config: None,
            },
        )
        .await
    }

    /// Asynchronously starts a command using structured options to avoid long arg lists
    pub async fn execute_async_in_dir_with_options<'a>(
        &self,
        tool_name: &str,
        command: &str,
        working_dir: &str,
        options: AsyncExecOptions<'a>,
    ) -> Result<String> {
        let AsyncExecOptions {
            operation_id,
            args,
            timeout,
            callback,
            subcommand_config,
        } = options;

        let op_id = operation_id.unwrap_or_else(generate_operation_id);
        let op_id_clone = op_id.clone();
        let wd = working_dir.to_string();

        let operation = Operation::new(
            op_id.clone(),
            tool_name.to_string(),
            format!("{} {:?}", command, args),
            None,
        );
        self.monitor.add_operation(operation).await;

        let monitor = self.monitor.clone();
        let shell_pool = self.shell_pool.clone();
        let command = command.to_string();
        let wd_clone = wd.clone();

        let (command_with_subcommand, args_vec) = self
            .prepare_command_and_args(&command, args.as_ref(), subcommand_config)
            .await?;

        let full_command = [command_with_subcommand]
            .iter()
            .map(ToString::to_string)
            .chain(args_vec.into_iter())
            .collect();

        let task_handles = self.task_handles.clone();

        let handle = tokio::spawn(async move {
            // Get the cancellation token from the operation
            let cancellation_token = {
                if let Some(operation) = monitor.get_operation(&op_id).await {
                    operation.cancellation_token.clone()
                } else {
                    tracing::error!("Could not find operation {} for cancellation token", op_id);
                    return;
                }
            };

            monitor
                .update_status(&op_id, OperationStatus::InProgress, None)
                .await;

            // Check for cancellation before starting
            if cancellation_token.is_cancelled() {
                tracing::info!("Operation {} was cancelled before execution started", op_id);
                monitor
                    .update_status(
                        &op_id,
                        OperationStatus::Cancelled,
                        Some(Value::String("Operation was cancelled".to_string())),
                    )
                    .await;
                // Notify LLM about cancellation with reason if available
                if let Some(callback) = &callback {
                    // Best effort: retrieve reason string from current operation state
                    let reason_owned = match monitor.get_operation(&op_id).await {
                        Some(op) => {
                            let val = op.result.clone();
                            val.and_then(|v| v.get("reason").cloned())
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                                .unwrap_or_else(|| "Operation was cancelled".to_string())
                        }
                        None => "Operation was cancelled".to_string(),
                    };
                    let _ = callback
                        .send_progress(crate::callback_system::ProgressUpdate::Cancelled {
                            operation_id: op_id.clone(),
                            message: reason_owned,
                            duration_ms: 0,
                        })
                        .await;
                }
                return;
            }

            let mut shell = match shell_pool.get_shell(&wd_clone).await {
                Some(s) => s,
                None => {
                    let error_message = "Failed to get shell from pool".to_string();
                    monitor
                        .update_status(
                            &op_id,
                            OperationStatus::Failed,
                            Some(Value::String(error_message.clone())),
                        )
                        .await;

                    // Send failure notification if callback is provided
                    if let Some(callback) = &callback {
                        let failure_update = crate::callback_system::ProgressUpdate::FinalResult {
                            operation_id: op_id.clone(),
                            command: command.clone(),
                            description: format!("Execute {} in {}", command, wd_clone),
                            working_directory: wd_clone.clone(),
                            success: false,
                            duration_ms: 0,
                            full_output: error_message,
                        };
                        if let Err(e) = callback.send_progress(failure_update).await {
                            tracing::error!("Failed to send failure notification: {:?}", e);
                        }
                    }
                    return;
                }
            };

            let shell_cmd = ShellCommand {
                id: op_id.clone(),
                command: full_command,
                working_dir: wd_clone.clone(),
                timeout_ms: timeout.map_or(
                    shell_pool.config().command_timeout.as_millis() as u64,
                    |t| t * 1000,
                ),
            };

            let start_time = std::time::Instant::now();

            // Check for cancellation before executing the command
            if cancellation_token.is_cancelled() {
                tracing::info!("Operation {} was cancelled before shell execution", op_id);
                monitor
                    .update_status(
                        &op_id,
                        OperationStatus::Cancelled,
                        Some(Value::String("Operation was cancelled".to_string())),
                    )
                    .await;
                if let Some(callback) = &callback {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    let reason_owned = match monitor.get_operation(&op_id).await {
                        Some(op) => {
                            let val = op.result.clone();
                            val.and_then(|v| v.get("reason").cloned())
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                                .unwrap_or_else(|| "Operation was cancelled".to_string())
                        }
                        None => "Operation was cancelled".to_string(),
                    };
                    let _ = callback
                        .send_progress(crate::callback_system::ProgressUpdate::Cancelled {
                            operation_id: op_id.clone(),
                            message: reason_owned,
                            duration_ms: elapsed,
                        })
                        .await;
                }
                shell_pool.return_shell(shell).await;
                return;
            }

            // DEBUGGING: Log shell command execution start
            tracing::debug!(
                "Starting shell execution for operation {} with command: {:?}",
                op_id,
                shell_cmd
            );

            let result = shell.execute_command(shell_cmd).await;

            // DEBUGGING: Log shell command execution completion
            tracing::debug!(
                "Shell execution completed for operation {} with result: {:?}",
                op_id,
                match &result {
                    Ok(output) => format!(
                        "Success(exit_code={}, stdout_len={}, stderr_len={})",
                        output.exit_code,
                        output.stdout.len(),
                        output.stderr.len()
                    ),
                    Err(e) => format!("Error: {}", e),
                }
            );

            let duration_ms = start_time.elapsed().as_millis() as u64;
            shell_pool.return_shell(shell).await;

            // Check for cancellation after command execution
            if cancellation_token.is_cancelled() {
                tracing::info!("Operation {} was cancelled after shell execution", op_id);
                monitor
                    .update_status(
                        &op_id,
                        OperationStatus::Cancelled,
                        Some(Value::String("Operation was cancelled".to_string())),
                    )
                    .await;
                if let Some(callback) = &callback {
                    let reason_owned = match monitor.get_operation(&op_id).await {
                        Some(op) => {
                            let val = op.result.clone();
                            val.and_then(|v| v.get("reason").cloned())
                                .and_then(|v| v.as_str().map(|s| s.to_string()))
                                .unwrap_or_else(|| "Operation was cancelled".to_string())
                        }
                        None => "Operation was cancelled".to_string(),
                    };
                    let _ = callback
                        .send_progress(crate::callback_system::ProgressUpdate::Cancelled {
                            operation_id: op_id.clone(),
                            message: reason_owned,
                            duration_ms,
                        })
                        .await;
                }
                return;
            }

            match result {
                Ok(output) => {
                    // DEBUGGING: Log all process output to trace cancellation source
                    tracing::debug!(
                        "Process output for operation {}: stdout='{}', stderr='{}', exit_code={}",
                        op_id,
                        output.stdout,
                        output.stderr,
                        output.exit_code
                    );

                    // Check if the output indicates cancellation - enhanced detection for rmcp library errors
                    let stdout_trimmed = output.stdout.trim();
                    let stderr_trimmed = output.stderr.trim();

                    let is_cancelled_output = stdout_trimmed == "Canceled"
                        || stderr_trimmed == "Canceled"
                        || output.stdout.contains("Canceled: Canceled")
                        || output.stderr.contains("Canceled: Canceled")
                        || output.stdout.contains("task cancelled for reason")
                        || output.stderr.contains("task cancelled for reason");

                    if is_cancelled_output {
                        // DEBUGGING: Log exactly what cancellation output was detected
                        tracing::debug!(
                            "CANCELLATION DETECTED for operation {}: stdout_contains_canceled_canceled={}, stderr_contains_canceled_canceled={}, stdout_contains_task_cancelled={}, stderr_contains_task_cancelled={}, stdout_exact_canceled={}, stderr_exact_canceled={}",
                            op_id,
                            output.stdout.contains("Canceled: Canceled"),
                            output.stderr.contains("Canceled: Canceled"),
                            output.stdout.contains("task cancelled for reason"),
                            output.stderr.contains("task cancelled for reason"),
                            stdout_trimmed == "Canceled",
                            stderr_trimmed == "Canceled"
                        );

                        // Enhanced cancellation handling: detect and transform rmcp library cancellation messages
                        tracing::info!("Detected cancelled process output for operation {}", op_id);

                        // First check if the operation already has a cancellation reason
                        let enhanced_reason = match monitor.get_operation(&op_id).await {
                            Some(op) if op.result.is_some() => {
                                // Operation already has a structured cancellation reason
                                if let Some(result) = &op.result {
                                    if let Some(reason) =
                                        result.get("reason").and_then(|v| v.as_str())
                                    {
                                        format!("Process cancelled: {}", reason)
                                    } else {
                                        "Process cancelled by external signal or user request"
                                            .to_string()
                                    }
                                } else {
                                    "Process cancelled by external signal or user request"
                                        .to_string()
                                }
                            }
                            _ => {
                                // No structured reason available, enhance based on output patterns
                                if output.stdout.contains("Canceled: Canceled")
                                    || output.stderr.contains("Canceled: Canceled")
                                {
                                    // This is the classic rmcp library "Canceled: Canceled" message
                                    "Operation cancelled by user request or system signal (was: Canceled: Canceled from rmcp library)".to_string()
                                } else if output.stdout.contains("task cancelled for reason")
                                    || output.stderr.contains("task cancelled for reason")
                                {
                                    // This is another rmcp library cancellation format
                                    "Operation cancelled by user request or system signal (detected rmcp library cancellation)".to_string()
                                } else {
                                    // Simple "Canceled" output from external process
                                    "Process cancelled by external signal or user request"
                                        .to_string()
                                }
                            }
                        };

                        monitor
                            .update_status(
                                &op_id,
                                OperationStatus::Cancelled,
                                Some(Value::String(enhanced_reason.clone())),
                            )
                            .await;

                        if let Some(callback) = &callback {
                            let _ = callback
                                .send_progress(crate::callback_system::ProgressUpdate::Cancelled {
                                    operation_id: op_id.clone(),
                                    message: enhanced_reason,
                                    duration_ms,
                                })
                                .await;
                        }
                        return;
                    }

                    let final_output = json!({
                        "stdout": output.stdout,
                        "stderr": output.stderr,
                        "exit_code": output.exit_code,
                    });
                    let success = output.exit_code == 0;
                    let status = if success {
                        OperationStatus::Completed
                    } else {
                        OperationStatus::Failed
                    };
                    monitor
                        .update_status(&op_id, status, Some(final_output.clone()))
                        .await;

                    // Send completion notification if callback is provided
                    if let Some(callback) = &callback {
                        let completion_update =
                            crate::callback_system::ProgressUpdate::FinalResult {
                                operation_id: op_id.clone(),
                                command: command.clone(),
                                description: format!("Execute {} in {}", command, wd_clone),
                                working_directory: wd_clone.clone(),
                                success,
                                duration_ms,
                                full_output: format!(
                                    "Exit code: {}\nStdout:\n{}\nStderr:\n{}",
                                    output.exit_code, output.stdout, output.stderr
                                ),
                            };
                        if let Err(e) = callback.send_progress(completion_update).await {
                            tracing::error!("Failed to send completion notification: {:?}", e);
                        } else {
                            tracing::info!("Sent completion notification for operation: {}", op_id);
                        }
                    }
                }
                Err(e) => {
                    let error_message = e.to_string();

                    // TIMEOUT FIX: Detect shell timeout errors and handle them as cancellations
                    let is_timeout_error = error_message.contains("Shell communication timeout")
                        || error_message.contains("timeout")
                        || error_message.contains("Timeout");

                    if is_timeout_error {
                        // Handle timeout as a cancellation with descriptive reason
                        let timeout_reason = format!(
                            "Operation timed out after {}ms (exceeded shell timeout limit)",
                            duration_ms
                        );

                        tracing::info!(
                            "Detected timeout for operation {}: {}",
                            op_id,
                            timeout_reason
                        );

                        monitor
                            .update_status(
                                &op_id,
                                OperationStatus::Cancelled,
                                Some(Value::String(timeout_reason.clone())),
                            )
                            .await;

                        // Send cancellation notification if callback is provided
                        if let Some(callback) = &callback {
                            let _ = callback
                                .send_progress(crate::callback_system::ProgressUpdate::Cancelled {
                                    operation_id: op_id.clone(),
                                    message: timeout_reason,
                                    duration_ms,
                                })
                                .await;
                        }
                    } else {
                        // Handle other errors as failures
                        monitor
                            .update_status(
                                &op_id,
                                OperationStatus::Failed,
                                Some(Value::String(error_message.clone())),
                            )
                            .await;

                        // Send failure notification if callback is provided
                        if let Some(callback) = &callback {
                            let failure_update =
                                crate::callback_system::ProgressUpdate::FinalResult {
                                    operation_id: op_id.clone(),
                                    command: command.clone(),
                                    description: format!("Execute {} in {}", command, wd_clone),
                                    working_directory: wd_clone.clone(),
                                    success: false,
                                    duration_ms,
                                    full_output: format!("Error: {}", error_message),
                                };
                            if let Err(e) = callback.send_progress(failure_update).await {
                                tracing::error!("Failed to send failure notification: {:?}", e);
                            } else {
                                tracing::info!(
                                    "Sent failure notification for operation: {}",
                                    op_id
                                );
                            }
                        }
                    }
                }
            }
            // Remove the task handle from the map once it's complete
            task_handles.lock().await.remove(&op_id);
        });

        // Store the handle for graceful shutdown
        self.task_handles
            .lock()
            .await
            .insert(op_id_clone.clone(), handle);

        Ok(op_id_clone)
    }

    async fn prepare_command_and_args(
        &self,
        base_command: &str,
        args: Option<&Map<String, serde_json::Value>>,
        subcommand_config: Option<&crate::config::SubcommandConfig>,
    ) -> Result<(String, Vec<String>)> {
        let mut command_parts: Vec<String> = base_command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let mut command_args = Vec::new();
        let mut processed_args = HashSet::new();

        if let Some(config) = subcommand_config {
            // Handle positional arguments in the order they are defined
            if let Some(positional_args) = &config.positional_args {
                for pos_arg_config in positional_args {
                    if let Some(value) = args.and_then(|a| a.get(&pos_arg_config.name)) {
                        if let Some(s) = value.as_str() {
                            command_args.push(s.to_string());
                        } else if let Some(arr) = value.as_array() {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    command_args.push(s.to_string());
                                }
                            }
                        } else if !value.is_null() {
                            command_args.push(value.to_string().trim_matches('"').to_string());
                        }
                        processed_args.insert(pos_arg_config.name.clone());
                    }
                }
            }

            // Handle named options
            if let Some(args_map) = args {
                if let Some(options) = &config.options {
                    for opt_config in options {
                        // Check for the option by its name or alias
                        let arg_value = args_map.get(&opt_config.name).or_else(|| {
                            opt_config
                                .alias
                                .as_ref()
                                .and_then(|alias| args_map.get(alias))
                        });

                        if let Some(value) = arg_value {
                            let flag = if opt_config.name.starts_with('-') {
                                opt_config.name.clone()
                            } else if opt_config.name.len() == 1 {
                                format!("-{}", opt_config.name)
                            } else {
                                format!("--{}", opt_config.name)
                            };

                            match opt_config.option_type.as_str() {
                                "boolean" => {
                                    let is_true = value.as_bool().unwrap_or_else(|| {
                                        value
                                            .as_str()
                                            .map(|s| s.to_lowercase() == "true")
                                            .unwrap_or(false)
                                    });
                                    if is_true {
                                        command_args.push(flag);
                                    }
                                }
                                "array" => {
                                    if let Some(arr) = value.as_array() {
                                        for item in arr {
                                            command_args.push(flag.clone());
                                            if let Some(s) = item.as_str() {
                                                command_args.push(s.to_string());
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    if let Some(s) = value.as_str() {
                                        // Check if this option supports file-based arguments and the string needs special handling
                                        if opt_config.file_arg.unwrap_or(false)
                                            && Self::needs_file_handling(s)
                                        {
                                            // Use file-based argument passing
                                            let temp_file_path =
                                                self.create_temp_file_with_content(s).await?;
                                            let file_flag =
                                                opt_config.file_flag.as_ref().unwrap_or(&flag);
                                            command_args.push(file_flag.clone());
                                            command_args.push(temp_file_path);
                                        } else if Self::needs_file_handling(s) {
                                            // Fall back to safe escaping if file handling not supported
                                            command_args.push(flag);
                                            command_args.push(Self::escape_shell_argument(s));
                                        } else {
                                            // Normal argument handling
                                            command_args.push(flag);
                                            command_args.push(s.to_string());
                                        }
                                    } else if !value.is_null() {
                                        command_args.push(flag);
                                        command_args
                                            .push(value.to_string().trim_matches('"').to_string());
                                    }
                                }
                            }
                            processed_args.insert(opt_config.name.clone());
                            if let Some(alias) = &opt_config.alias {
                                processed_args.insert(alias.clone());
                            }
                        }
                    }
                }
            }
        }

        // Fallback for arguments not covered by the config or when no config is provided
        if let Some(args_map) = args {
            for (key, value) in args_map {
                if processed_args.contains(key) || key == "working_directory" {
                    continue;
                }

                let flag = if key.starts_with('-') {
                    key.clone()
                } else if key.len() == 1 {
                    format!("-{}", key)
                } else {
                    format!("--{}", key)
                };

                if let Some(b) = value.as_bool() {
                    if b {
                        command_args.push(flag);
                    }
                } else if let Some(s) = value.as_str() {
                    let lower_s = s.to_lowercase();
                    if lower_s == "true" {
                        command_args.push(flag);
                    } else if lower_s != "false" {
                        command_args.push(flag);
                        // For fallback arguments, use safe escaping for problematic strings
                        if Self::needs_file_handling(s) {
                            command_args.push(Self::escape_shell_argument(s));
                        } else {
                            command_args.push(s.to_string());
                        }
                    }
                } else if let Some(arr) = value.as_array() {
                    for item in arr {
                        command_args.push(flag.clone());
                        if let Some(s) = item.as_str() {
                            command_args.push(s.to_string());
                        }
                    }
                } else if !value.is_null() {
                    command_args.push(flag);
                    command_args.push(value.to_string().trim_matches('"').to_string());
                }
            }
        }

        let final_command = command_parts.remove(0);
        command_parts.extend(command_args);
        Ok((final_command, command_parts))
    }

    /// Checks if a string contains characters that are problematic for shell argument passing
    pub fn needs_file_handling(value: &str) -> bool {
        value.contains('\n')
            || value.contains('\r')
            || value.contains('\'')
            || value.contains('"')
            || value.contains('\\')
            || value.contains('`')
            || value.contains('$')
            || value.len() > 8192 // Also handle very long arguments via file
    }

    /// Creates a temporary file with the given content and returns the file path
    async fn create_temp_file_with_content(&self, content: &str) -> Result<String> {
        let mut temp_file = NamedTempFile::new()
            .context("Failed to create temporary file for multi-line argument")?;

        temp_file
            .write_all(content.as_bytes())
            .context("Failed to write content to temporary file")?;

        let file_path = temp_file.path().to_string_lossy().to_string();

        // Store the temp file so it doesn't get cleaned up until the adapter is dropped
        {
            let mut temp_files = self.temp_files.lock().await;
            temp_files.push(temp_file);
        }

        Ok(file_path)
    }

    /// Safely escapes a string for shell argument passing as a fallback when file handling isn't available
    pub fn escape_shell_argument(value: &str) -> String {
        // Use single quotes and escape any embedded single quotes
        if value.contains('\'') {
            format!("'{}'", value.replace('\'', "'\"'\"'"))
        } else {
            format!("'{}'", value)
        }
    }
}
