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
    shell_pool::ShellPoolManager,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;
use tokio::{sync::Mutex, task::JoinHandle};

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

    /// Gracefully shuts down the adapter by cancelling active operations, aborting tasks,
    /// and shutting down shell pools. Uses timeouts to avoid hanging indefinitely.
    pub async fn shutdown(&self) {
        tracing::info!("Adapter shutdown initiated: cancelling operations and aborting tasks");

        // 1) Cancel all known operations tracked by this adapter (best-effort)
        // We use the task handle keys which are the operation IDs
        {
            let handles = self.task_handles.lock().await;
            for op_id in handles.keys() {
                // Provide a clear reason for downstream logs
                let reason = Some("Adapter shutdown".to_string());
                let _ = self
                    .monitor
                    .cancel_operation_with_reason(op_id, reason)
                    .await;
            }
        }

        // 2) Abort all running tasks and wait briefly for them to finish
        // Drain handles to avoid races with task completion removing them concurrently
        let mut drained: Vec<(String, JoinHandle<()>)> = Vec::new();
        {
            let mut handles = self.task_handles.lock().await;
            for (id, handle) in handles.drain() {
                drained.push((id, handle));
            }
        }

        // First give a small grace period for tasks to finish naturally
        for (id, handle) in drained.iter_mut() {
            tracing::debug!("Waiting briefly for task {} to complete...", id);
            match tokio::time::timeout(Duration::from_millis(250), handle).await {
                Ok(res) => {
                    if let Err(e) = res {
                        tracing::debug!(
                            "Task {} finished with join error before abort: {:?}",
                            id,
                            e
                        );
                    }
                }
                Err(_) => {
                    tracing::debug!("Task {} did not complete in grace period", id);
                }
            }
        }

        // Abort any remaining tasks and await their termination with a bounded timeout
        for (id, mut handle) in drained {
            if !handle.is_finished() {
                tracing::info!("Aborting task {} during shutdown", id);
                handle.abort();
                // Await aborted join to ensure cleanup
                match tokio::time::timeout(Duration::from_secs(2), &mut handle).await {
                    Ok(join_res) => {
                        if let Err(e) = join_res {
                            tracing::debug!("Task {} aborted with: {:?}", id, e);
                        }
                    }
                    Err(_) => {
                        tracing::warn!("Timed out waiting for aborted task {} to finish", id);
                    }
                }
            }
        }

        // 3) Shut down all shell pools (kills any lingering shell processes)
        tracing::info!("Shutting down shell pools");
        self.shell_pool.shutdown_all().await;

        tracing::info!("Adapter shutdown complete");
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
        let (program, args_vec) = self
            .prepare_command_and_args(command, args.as_ref(), subcommand_config)
            .await?;

        let timeout = timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or_else(|| self.shell_pool.config().command_timeout);

        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args_vec)
            .current_dir(working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output_res = tokio::time::timeout(timeout, cmd.output()).await;

        match output_res {
            Err(_) => Err(anyhow::anyhow!(
                "Operation timed out (exceeded timeout limit): {} seconds",
                timeout.as_secs()
            )),
            Ok(Err(e)) => Err(anyhow::anyhow!("Command execution failed: {}", e)),
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.success() {
                    let result_content = if stdout.is_empty() && !stderr.is_empty() {
                        stderr
                    } else if !stdout.is_empty() && !stderr.is_empty() {
                        format!("{}\n{}", stdout, stderr)
                    } else {
                        stdout
                    };
                    Ok(result_content)
                } else {
                    Err(anyhow::anyhow!(
                        "Command failed with exit code {}: stderr: {}, stdout: {}",
                        output.status.code().unwrap_or(-1),
                        stderr,
                        stdout
                    ))
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
    pub async fn execute_async_in_dir_with_options(
        &self,
        tool_name: &str,
        command: &str,
        working_dir: &str,
        options: AsyncExecOptions<'_>,
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

        let timeout_duration = timeout.map(Duration::from_secs);

        let operation = Operation::new_with_timeout(
            op_id.clone(),
            tool_name.to_string(),
            format!("{} {:?}", command, args),
            None,
            timeout_duration,
        );
        self.monitor.add_operation(operation).await;

        let monitor = self.monitor.clone();
        let shell_pool = self.shell_pool.clone();
        let command = command.to_string();
        let wd_clone = wd.clone();

        let (program_with_subcommand, args_vec) = self
            .prepare_command_and_args(&command, args.as_ref(), subcommand_config)
            .await?;

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

            // Send an immediate 'Started' notification if a callback is provided
            if let Some(callback) = &callback {
                let _ = callback
                    .send_progress(crate::callback_system::ProgressUpdate::Started {
                        operation_id: op_id.clone(),
                        command: command.clone(),
                        description: format!(
                            "Execute {} in {}",
                            command, wd_clone
                        ),
                    })
                    .await;
            }

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

            let start_time = Instant::now();

            // Build process command to execute directly without shell pool dependency
            let mut proc_cmd = tokio::process::Command::new(&program_with_subcommand);
            proc_cmd
                .args(&args_vec)
                .current_dir(&wd_clone)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            // Resolve timeout in milliseconds
            let timeout_ms: u64 = timeout
                .map(|t| t * 1000)
                .unwrap_or_else(|| shell_pool.config().command_timeout.as_millis() as u64);

            // Execute with timeout
            let proc_result = tokio::time::timeout(Duration::from_millis(timeout_ms), proc_cmd.output()).await;

            let duration_ms = start_time.elapsed().as_millis() as u64;

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

            match proc_result {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let final_output = json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": output.status.code().unwrap_or(-1),
                    });
                    let success = output.status.success();
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
                                command: program_with_subcommand.clone(),
                                description: format!(
                                    "Execute {} in {}",
                                    program_with_subcommand, wd_clone
                                ),
                                working_directory: wd_clone.clone(),
                                success,
                                duration_ms,
                                full_output: format!(
                                    "Exit code: {}\nStdout:\n{}\nStderr:\n{}",
                                    final_output["exit_code"], final_output["stdout"], final_output["stderr"]
                                ),
                            };
                        if let Err(e) = callback.send_progress(completion_update).await {
                            tracing::error!("Failed to send completion notification: {:?}", e);
                        } else {
                            tracing::info!("Sent completion notification for operation: {}", op_id);
                        }
                    }
                }
                Ok(Err(e)) => {
                    let error_message = e.to_string();
                    monitor
                        .update_status(
                            &op_id,
                            OperationStatus::Failed,
                            Some(Value::String(error_message.clone())),
                        )
                        .await;

                    if let Some(callback) = &callback {
                        let failure_update = crate::callback_system::ProgressUpdate::FinalResult {
                            operation_id: op_id.clone(),
                            command: program_with_subcommand.clone(),
                            description: format!(
                                "Execute {} in {}",
                                program_with_subcommand, wd_clone
                            ),
                            working_directory: wd_clone.clone(),
                            success: false,
                            duration_ms,
                            full_output: format!("Error: {}", error_message),
                        };
                        let _ = callback.send_progress(failure_update).await;
                    }
                }
                Err(_) => {
                    // Timeout
                    let timeout_reason = format!(
                        "Operation timed out after {}ms (exceeded timeout limit)",
                        duration_ms
                    );
                    monitor
                        .update_status(
                            &op_id,
                            OperationStatus::Cancelled,
                            Some(Value::String(timeout_reason.clone())),
                        )
                        .await;

                    if let Some(callback) = &callback {
                        let _ = callback
                            .send_progress(crate::callback_system::ProgressUpdate::Cancelled {
                                operation_id: op_id.clone(),
                                message: timeout_reason,
                                duration_ms,
                            })
                            .await;
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

        // Perform the blocking write on Tokio's blocking thread pool.
        let temp_file = {
            // Move the NamedTempFile into the blocking task and return it after write.
            let content = content.to_owned();
            tokio::task::spawn_blocking(move || -> Result<NamedTempFile> {
                temp_file
                    .write_all(content.as_bytes())
                    .context("Failed to write content to temporary file")?;
                temp_file
                    .flush()
                    .context("Failed to flush temporary file")?;
                Ok(temp_file)
            })
            .await
            .context("Failed to run blocking write in background")??
        };

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
