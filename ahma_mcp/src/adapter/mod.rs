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
//!
//! ## Security Note
//! Kernel-enforced sandboxing is the only security boundary (R7). The adapter does not
//! attempt to parse command strings for security.

mod preparer;
mod types;

pub use preparer::{escape_shell_argument, format_option_flag, needs_file_handling};
pub use types::{AsyncExecOptions, ExecutionMode};

use crate::operation_monitor::{Operation, OperationMonitor, OperationStatus};
use crate::retry::{self, RetryConfig};
use crate::sandbox;
use crate::shell_pool::ShellPoolManager;
use anyhow::Result;
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{sync::Mutex, task::JoinHandle};

static OPERATION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_operation_id() -> String {
    let id = OPERATION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("op_{}", id)
}

/// The core execution engine for external tools.
///
/// The `Adapter` coordinates the execution of CLI commands, managing the lifecycle of
/// shell processes, operation tracking, and resource cleanup. It serves as the bridge
/// between incoming tool requests and the underlying system shell.
///
/// # Responsibilities
///
/// *   **Command Execution**: Executes tools using either a pre-warmed shell pool (for async
///     performance) or standard process spawning.
/// *   **Resource Management**: Manages temporary files created for complex arguments and
///     ensures they are cleaned up.
/// *   **Shell Pooling**: Integrates with `ShellPoolManager` to reuse shell processes,
///     reducing latency for frequent commands.
/// *   **Operation Tracking**: Uses `OperationMonitor` to track the status (running, completed,
///     failed) of asynchronous operations.
/// *   **Sandboxing**: Enforces path security by validating operations against a root directory.
/// *   **Retry Logic**: Applies configured retry policies for transient failures.
///
/// # Thread Safety
///
/// The `Adapter` is designed to be shared across threads (`Arc<Adapter>`) and uses internal
/// synchronization to manage state safely.
#[derive(Debug)]
pub struct Adapter {
    /// Operation monitor for async tasks.
    monitor: Arc<OperationMonitor>,
    /// Pre-warmed shell pool manager for async execution.
    shell_pool: Arc<ShellPoolManager>,
    /// Security sandbox context.
    sandbox: Arc<sandbox::Sandbox>,
    /// Handles to spawned tasks for graceful shutdown.
    task_handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// Temporary file manager for multi-line arguments - cleaned up automatically when dropped
    temp_file_manager: preparer::TempFileManager,
    /// Optional retry configuration for transient error handling.
    retry_config: Option<RetryConfig>,
}

impl Adapter {
    /// Creates a new `Adapter` instance.
    ///
    /// The adapter requires an `OperationMonitor` for tracking async tasks, a `ShellPoolManager`
    /// for efficient shell execution, and a `Sandbox` for security context.
    ///
    /// # Arguments
    ///
    /// * `monitor` - Shared reference to the operation monitor
    /// * `shell_pool` - Shared reference to the shell pool manager
    /// * `sandbox` - Shared reference to the security sandbox
    pub fn new(
        monitor: Arc<OperationMonitor>,
        shell_pool: Arc<ShellPoolManager>,
        sandbox: Arc<sandbox::Sandbox>,
    ) -> Result<Self> {
        Ok(Self {
            monitor,
            shell_pool,
            sandbox,
            task_handles: Arc::new(Mutex::new(HashMap::new())),
            temp_file_manager: preparer::TempFileManager::new(),
            retry_config: None,
        })
    }

    /// Sets retry configuration for transient error handling.
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = Some(config);
        self
    }

    /// Returns the retry configuration, if set.
    pub fn retry_config(&self) -> Option<&RetryConfig> {
        self.retry_config.as_ref()
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

    /// Get a reference to the sandbox.
    pub fn sandbox(&self) -> &crate::sandbox::Sandbox {
        &self.sandbox
    }

    /// Synchronously executes a command and returns the result directly.
    ///
    /// This method bypasses the async operation queue and runs the command directly, waiting for it to complete.
    /// It captures and returns the combined stdout and stderr.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute (e.g., "ls", "grep").
    /// * `args` - Optional map of arguments (see `prepare_command_and_args`).
    /// * `working_dir` - Directory to execute the command in. Must be within allowed sandbox scopes.
    /// * `timeout_seconds` - Optional timeout in seconds (overrides default).
    /// * `subcommand_config` - Optional configuration for dealing with subcommands and aliases.
    ///
    /// # Returns
    ///
    /// Returns the command output (stdout + stderr) as a `String` if successful.
    /// Returns an error if execution fails or times out.
    pub async fn execute_sync_in_dir(
        &self,
        command: &str,
        args: Option<Map<String, serde_json::Value>>,
        working_dir: &str,
        timeout_seconds: Option<u64>,
        subcommand_config: Option<&crate::config::SubcommandConfig>,
    ) -> Result<String, anyhow::Error> {
        tracing::error!(
            "execute_sync_in_dir START: command='{}', working_dir='{}', args={:?}",
            command,
            working_dir,
            args
        );

        // Validate working directory against sandbox scope.
        let safe_wd = self
            .sandbox
            .validate_path(std::path::Path::new(working_dir))?;

        let (program, args_vec) = self
            .prepare_command_and_args(command, args.as_ref(), subcommand_config, &safe_wd)
            .await?;

        tracing::info!(
            "Prepared command: program='{}', args={:?}",
            program,
            args_vec
        );

        let timeout = timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or_else(|| self.shell_pool.config().command_timeout);

        // Create sandboxed command
        let mut cmd = if program == "/bin/sh" {
            let full_command = args_vec.join(" ");
            self.sandbox
                .create_shell_command(&program, &full_command, &safe_wd)?
        } else {
            self.sandbox.create_command(&program, &args_vec, &safe_wd)?
        };

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

    /// Synchronously executes a command with optional retry logic for transient errors.
    ///
    /// If a `RetryConfig` is set on the adapter, transient errors (timeouts, resource
    /// exhaustion, network issues) will be retried with exponential backoff.
    /// Permanent errors (command not found, permission denied) fail immediately.
    ///
    /// If no retry config is set, this behaves identically to `execute_sync_in_dir`.
    pub async fn execute_sync_with_retry(
        &self,
        command: &str,
        args: Option<Map<String, serde_json::Value>>,
        working_dir: &str,
        timeout_seconds: Option<u64>,
        subcommand_config: Option<&crate::config::SubcommandConfig>,
    ) -> Result<String, anyhow::Error> {
        match &self.retry_config {
            Some(config) => {
                // Clone args for each retry attempt since the closure needs ownership
                let args_clone = args.clone();
                retry::execute_with_retry(config, || {
                    let args_inner = args_clone.clone();
                    async move {
                        self.execute_sync_in_dir(
                            command,
                            args_inner,
                            working_dir,
                            timeout_seconds,
                            subcommand_config,
                        )
                        .await
                    }
                })
                .await
            }
            None => {
                // No retry config, execute directly
                self.execute_sync_in_dir(
                    command,
                    args,
                    working_dir,
                    timeout_seconds,
                    subcommand_config,
                )
                .await
            }
        }
    }

    /// Asynchronously starts a command, returning an operation ID immediately.
    ///
    /// This method queues the command for execution in a background task, potentially using a
    /// pre-warmed shell from the pool. The result will be reported via the `OperationMonitor`
    /// and any registered callbacks.
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The logical name of the tool (for logging/monitoring).
    /// * `command` - The base command to run.
    /// * `args` - Optional arguments for the command.
    /// * `working_directory` - Directory to execute in.
    /// * `timeout` - Optional timeout in seconds.
    ///
    /// # Returns
    ///
    /// Returns a `String` containing the `operation_id` (e.g., "op_123").
    /// The actual command result is not returned here.
    pub async fn execute_async_in_dir(
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

        // Validate working directory against sandbox scope.
        let safe_wd = self
            .sandbox
            .validate_path(std::path::Path::new(working_dir))?;
        let safe_wd_str = safe_wd.to_string_lossy().to_string();

        // Validate command arguments
        let (program_with_subcommand, args_vec) = self
            .prepare_command_and_args(command, args.as_ref(), subcommand_config, &safe_wd)
            .await?;

        let op_id = operation_id.unwrap_or_else(generate_operation_id);
        let op_id_clone = op_id.clone();
        let wd = safe_wd_str.clone();

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
        let sandbox = self.sandbox.clone(); // Clone ARC to pass to task
        let command = command.to_string();
        let wd_clone = wd.clone();

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
                        description: format!("Execute {} in {}", command, wd_clone),
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

            // Build sandboxed process command
            let wd_path = std::path::PathBuf::from(&wd_clone);
            let proc_cmd_result =
                sandbox.create_command(&program_with_subcommand, &args_vec, &wd_path);

            let mut proc_cmd: tokio::process::Command = match proc_cmd_result {
                Ok(cmd) => cmd,
                Err(e) => {
                    let error_message = format!("Failed to create sandboxed command: {}", e);
                    tracing::error!("{}", error_message);
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
                            duration_ms: 0,
                            full_output: format!("Error: {}", error_message),
                        };
                        let _ = callback.send_progress(failure_update).await;
                    }
                    return;
                }
            };

            // Resolve timeout in milliseconds
            let timeout_ms: u64 = timeout
                .map(|t| t * 1000)
                .unwrap_or_else(|| shell_pool.config().command_timeout.as_millis() as u64);

            // Execute with timeout
            let proc_result =
                tokio::time::timeout(Duration::from_millis(timeout_ms), proc_cmd.output()).await;

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
                                    final_output["exit_code"],
                                    final_output["stdout"],
                                    final_output["stderr"]
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

    /// Parses the command string and arguments into a program and argument list.
    ///
    /// This helper handles complications such as:
    /// - Splitting the base command string (e.g., "python script.py" -> program: "python", args: ["script.py"])
    /// - Converting structured JSON arguments into CLI flags and positional arguments
    /// - Applying subcommand configurations (aliases, hardcoded args)
    /// - Creating temporary files for multi-line string arguments
    async fn prepare_command_and_args(
        &self,
        command: &str,
        args: Option<&Map<String, Value>>,
        subcommand_config: Option<&crate::config::SubcommandConfig>,
        working_dir: &std::path::Path,
    ) -> Result<(String, Vec<String>)> {
        preparer::prepare_command_and_args(
            command,
            args,
            subcommand_config,
            working_dir,
            &self.temp_file_manager,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    use serde_json::json;
    use std::path::Path;

    fn test_adapter() -> Arc<Adapter> {
        test_utils::client::create_test_config(Path::new(".")).expect("adapter")
    }

    #[tokio::test]
    async fn shell_commands_append_redirect_once() {
        let adapter = test_adapter();
        let mut args_map = Map::new();
        args_map.insert("args".to_string(), json!(["echo hi"]));

        let (program, args_vec) = adapter
            .prepare_command_and_args("/bin/sh -c", Some(&args_map), None, Path::new("."))
            .await
            .expect("command");

        assert_eq!(program, "/bin/sh");
        assert_eq!(args_vec, vec!["-c".to_string(), "echo hi 2>&1".to_string()]);
    }

    #[tokio::test]
    async fn shell_commands_do_not_duplicate_redirect() {
        let adapter = test_adapter();
        let mut args_map = Map::new();
        args_map.insert("args".to_string(), json!(["ls 2>&1"]));

        let (_, args_vec) = adapter
            .prepare_command_and_args("/bin/sh -c", Some(&args_map), None, Path::new("."))
            .await
            .expect("command");

        assert_eq!(args_vec, vec!["-c".to_string(), "ls 2>&1".to_string()]);
    }

    #[tokio::test]
    async fn non_shell_commands_remain_unchanged() {
        let adapter = test_adapter();
        let mut args_map = Map::new();
        args_map.insert("args".to_string(), json!(["--version"]));

        let (program, args_vec) = adapter
            .prepare_command_and_args("git", Some(&args_map), None, Path::new("."))
            .await
            .expect("command");

        assert_eq!(program, "git");
        assert_eq!(args_vec, vec!["--version".to_string()]);
    }

    #[test]
    fn format_option_flag_standard_option() {
        // Standard options get -- prefix
        assert_eq!(preparer::format_option_flag("verbose"), "--verbose");
        assert_eq!(preparer::format_option_flag("force"), "--force");
        assert_eq!(
            preparer::format_option_flag("working_directory"),
            "--working_directory"
        );
    }

    #[test]
    fn format_option_flag_dash_prefixed_option() {
        // Options already starting with - are used as-is
        assert_eq!(preparer::format_option_flag("-name"), "-name");
        assert_eq!(preparer::format_option_flag("-type"), "-type");
        assert_eq!(preparer::format_option_flag("-mtime"), "-mtime");
        // Double-dash options are also preserved
        assert_eq!(preparer::format_option_flag("--version"), "--version");
    }

    #[test]
    fn format_option_flag_empty_string() {
        // Empty string should get -- prefix (edge case)
        assert_eq!(preparer::format_option_flag(""), "--");
    }

    #[tokio::test]
    async fn find_command_args_with_dash_prefix() {
        use crate::config::{CommandOption, SubcommandConfig};

        let adapter = test_adapter();

        // Create a subcommand config that matches the find subcommand in file_tools.json
        let subcommand_config = SubcommandConfig {
            name: "find".to_string(),
            description: "Search for files".to_string(),
            enabled: true,
            positional_args_first: Some(true), // find requires path before options
            positional_args: Some(vec![CommandOption {
                name: "path".to_string(),
                description: None,
                required: Some(false),
                option_type: "string".to_string(),
                format: Some("path".to_string()),
                items: None,
                file_arg: None,
                file_flag: None,
                alias: None,
            }]),
            options: Some(vec![
                CommandOption {
                    name: "-name".to_string(),
                    option_type: "string".to_string(),
                    description: Some("Search pattern".to_string()),
                    required: None,
                    format: None,
                    items: None,
                    file_arg: None,
                    file_flag: None,
                    alias: None,
                },
                CommandOption {
                    name: "-maxdepth".to_string(),
                    option_type: "integer".to_string(),
                    description: Some("Max depth".to_string()),
                    required: None,
                    format: None,
                    items: None,
                    file_arg: None,
                    file_flag: None,
                    alias: None,
                },
            ]),
            subcommand: None,
            timeout_seconds: None,
            synchronous: None,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        };

        let mut args_map = Map::new();
        args_map.insert("path".to_string(), json!("."));
        args_map.insert("-name".to_string(), json!("*.toml"));
        args_map.insert("-maxdepth".to_string(), json!(1));

        let (program, args_vec) = adapter
            .prepare_command_and_args(
                "find",
                Some(&args_map),
                Some(&subcommand_config),
                Path::new("."),
            )
            .await
            .expect("command");

        assert_eq!(program, "find");
        // With positional_args_first: true, path should come BEFORE options
        // This is required by both BSD and GNU find
        assert!(
            args_vec.contains(&"-name".to_string()),
            "Should contain -name, got: {:?}",
            args_vec
        );
        assert!(
            args_vec.contains(&"-maxdepth".to_string()),
            "Should contain -maxdepth, got: {:?}",
            args_vec
        );
        assert!(
            args_vec.contains(&"*.toml".to_string()),
            "Should contain pattern value, got: {:?}",
            args_vec
        );
        assert!(
            args_vec.contains(&"1".to_string()),
            "Should contain depth value, got: {:?}",
            args_vec
        );
        // With positional_args_first: true, the path should be the first argument
        // (path is expanded to absolute path due to format: "path")
        let first_arg = args_vec.first().expect("Should have at least one argument");
        assert!(
            first_arg.starts_with('/') || first_arg == ".",
            "First argument should be a path, got: {:?}",
            args_vec
        );
        // Verify path comes before options
        let name_idx = args_vec.iter().position(|s| s == "-name").unwrap();
        let maxdepth_idx = args_vec.iter().position(|s| s == "-maxdepth").unwrap();
        assert!(
            0 < name_idx && 0 < maxdepth_idx,
            "Path (index 0) should come before options (-name at {}, -maxdepth at {}): {:?}",
            name_idx,
            maxdepth_idx,
            args_vec
        );
        // Should NOT contain --maxdepth or ---name
        assert!(
            !args_vec.iter().any(|s| s == "--maxdepth"),
            "Should NOT contain --maxdepth, got: {:?}",
            args_vec
        );
        assert!(
            !args_vec.iter().any(|s| s == "---name"),
            "Should NOT contain ---name, got: {:?}",
            args_vec
        );
    }
}
