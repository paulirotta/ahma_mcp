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
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
}

impl Adapter {
    /// Create a new adapter with a specific timeout.
    pub fn new(monitor: Arc<OperationMonitor>, shell_pool: Arc<ShellPoolManager>) -> Result<Self> {
        Ok(Self {
            monitor,
            shell_pool,
            task_handles: Arc::new(Mutex::new(HashMap::new())),
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

        let (command_with_subcommand, args_vec) =
            self.prepare_command_and_args(command, args.as_ref(), subcommand_config);

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
            Err(e) => Err(anyhow::anyhow!("Command execution failed: {}", e)),
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
    ) -> String {
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
    ) -> String {
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
    ) -> String {
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

        let (command_with_subcommand, args_vec) =
            self.prepare_command_and_args(&command, args.as_ref(), subcommand_config);

        let full_command = [command_with_subcommand]
            .iter()
            .map(ToString::to_string)
            .chain(args_vec.into_iter())
            .collect();

        let task_handles = self.task_handles.clone();

        let handle = tokio::spawn(async move {
            monitor
                .update_status(&op_id, OperationStatus::InProgress, None)
                .await;

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
            let result = shell.execute_command(shell_cmd).await;
            let duration_ms = start_time.elapsed().as_millis() as u64;
            shell_pool.return_shell(shell).await;

            match result {
                Ok(output) => {
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
                            duration_ms,
                            full_output: format!("Error: {}", error_message),
                        };
                        if let Err(e) = callback.send_progress(failure_update).await {
                            tracing::error!("Failed to send failure notification: {:?}", e);
                        } else {
                            tracing::info!("Sent failure notification for operation: {}", op_id);
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

        op_id_clone
    }

    fn prepare_command_and_args(
        &self,
        base_command: &str,
        args: Option<&Map<String, serde_json::Value>>,
        subcommand_config: Option<&crate::config::SubcommandConfig>,
    ) -> (String, Vec<String>) {
        let mut command_args = Vec::new();
        let mut processed_args = HashSet::new();

        if let Some(config) = subcommand_config {
            // Handle subcommand if present in args
            if let Some(subcommand_val) = args.and_then(|a| a.get("_subcommand")) {
                if let Some(subcommand_str) = subcommand_val.as_str() {
                    command_args.push(subcommand_str.to_string());
                    processed_args.insert("_subcommand".to_string());
                }
            }

            // Handle positional arguments in the order they are defined
            for pos_arg_config in &config.positional_args {
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

            // Handle named options
            if let Some(args_map) = args {
                for opt_config in &config.options {
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
                                // Command line flag (present/absent) - like --cached, --verbose, --release
                                let is_true = value.as_bool().unwrap_or_else(|| {
                                    // Also handle string representations "true"/"false" from MCP client
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
                                // string, int, etc.
                                if let Some(s) = value.as_str() {
                                    command_args.push(flag);
                                    command_args.push(s.to_string());
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

        // Fallback for arguments not covered by the config or when no config is provided
        if let Some(args_map) = args {
            for (key, value) in args_map {
                if processed_args.contains(key) || key == "working_directory" {
                    continue;
                }

                // This part retains the old logic for args not in the config
                if key == "_subcommand" {
                    if let Some(subcommand) = value.as_str() {
                        // Insert subcommand at the beginning if not already there
                        if command_args.is_empty() || command_args[0] != subcommand {
                            command_args.insert(0, subcommand.to_string());
                        }
                    }
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
                    // Handle string representations of booleans
                    let lower_s = s.to_lowercase();
                    if lower_s == "true" {
                        command_args.push(flag);
                    } else if lower_s != "false" {
                        // Only add as key-value if it's not a boolean string
                        command_args.push(flag);
                        command_args.push(s.to_string());
                    }
                    // If it's "false", we don't add anything (flag is not set)
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

        // Special handling for `cargo nextest`
        // It's a separate binary that cargo knows how to invoke.
        // The command from MCP is `cargo nextest ...`, which is the correct invocation.
        // No special transformation is needed. The previous logic was creating an incorrect command.
        if base_command == "cargo" && command_args.first().is_some_and(|s| s == "nextest") {
            return (base_command.to_string(), command_args);
        }

        (base_command.to_string(), command_args)
    }
}
