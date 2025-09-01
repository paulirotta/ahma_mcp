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
use rmcp::ErrorData as McpError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    Synchronous,
    AsyncResultPush,
}

/// The main adapter that handles command execution.
#[derive(Debug)]
pub struct Adapter {
    /// Operation monitor for async tasks.
    monitor: Arc<OperationMonitor>,
    /// Pre-warmed shell pool manager for async execution.
    shell_pool: Arc<ShellPoolManager>,
}

impl Adapter {
    /// Create a new adapter with a specific timeout.
    pub fn new(monitor: Arc<OperationMonitor>, shell_pool: Arc<ShellPoolManager>) -> Result<Self> {
        Ok(Self {
            monitor,
            shell_pool,
        })
    }

    /// Synchronously executes a command and returns the result directly.
    pub async fn execute_sync_in_dir(
        &self,
        command: &str,
        args: Option<serde_json::Map<String, Value>>,
        working_directory: &str,
        timeout: Option<u64>,
    ) -> Result<String, McpError> {
        let mut shell = self
            .shell_pool
            .get_shell(working_directory)
            .await
            .ok_or_else(|| McpError::internal_error("shell_unavailable", None))?;

        let args_vec = args
            .map(|map| {
                let mut positional_args = Vec::new();
                let mut flag_args = Vec::new();

                for (k, v) in map.into_iter() {
                    if k == "_subcommand" {
                        // Handle subcommand as positional argument (first)
                        positional_args.insert(0, v.as_str().unwrap_or("").to_string());
                    } else {
                        // Handle regular arguments as flags
                        flag_args.push(format!("--{}={}", k, v.as_str().unwrap_or("")));
                    }
                }

                // Combine positional args first, then flag args
                positional_args
                    .into_iter()
                    .chain(flag_args.into_iter())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let shell_cmd = ShellCommand {
            id: Uuid::new_v4().to_string(),
            command: [command]
                .iter()
                .map(ToString::to_string)
                .chain(args_vec.into_iter())
                .collect(),
            working_dir: working_directory.to_string(),
            timeout_ms: timeout.map_or(
                self.shell_pool.config().command_timeout.as_millis() as u64,
                |t| t * 1000,
            ),
        };

        let result = shell.execute_command(shell_cmd).await;
        self.shell_pool.return_shell(shell).await;

        match result {
            Ok(output) => {
                if output.exit_code == 0 {
                    Ok(output.stdout)
                } else {
                    Err(McpError::internal_error(
                        "command_failed",
                        Some(json!({
                            "error": output.stderr,
                            "stdout": output.stdout,
                            "exit_code": output.exit_code
                        })),
                    ))
                }
            }
            Err(e) => Err(McpError::internal_error(
                "command_failed",
                Some(json!({"error": e.to_string()})),
            )),
        }
    }

    /// Asynchronously starts a command, returns a job_id, and pushes the result later.
    pub async fn execute_async_in_dir(
        &self,
        command: &str,
        args: Option<serde_json::Map<String, Value>>,
        working_directory: &str,
        timeout: Option<u64>,
    ) -> String {
        let op_id = Uuid::new_v4().to_string();
        let op_id_clone = op_id.clone();
        let wd = working_directory.to_string();

        let operation = Operation::new(op_id.clone(), format!("{} {:?}", command, args), None);
        self.monitor.add_operation(operation).await;

        let monitor = self.monitor.clone();
        let shell_pool = self.shell_pool.clone();
        let command = command.to_string();

        let args_vec = args
            .map(|map| {
                let mut positional_args = Vec::new();
                let mut flag_args = Vec::new();

                for (k, v) in map.into_iter() {
                    if k == "_subcommand" {
                        // Handle subcommand as positional argument (first)
                        positional_args.insert(0, v.as_str().unwrap_or("").to_string());
                    } else {
                        // Handle regular arguments as flags
                        flag_args.push(format!("--{}={}", k, v.as_str().unwrap_or("")));
                    }
                }

                // Combine positional args first, then flag args
                positional_args
                    .into_iter()
                    .chain(flag_args.into_iter())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let full_command = [command.clone()]
            .iter()
            .map(ToString::to_string)
            .chain(args_vec.into_iter())
            .collect();

        tokio::spawn(async move {
            monitor
                .update_status(&op_id, OperationStatus::InProgress, None)
                .await;

            let mut shell = match shell_pool.get_shell(&wd).await {
                Some(s) => s,
                None => {
                    let error_message = "Failed to get shell from pool".to_string();
                    monitor
                        .update_status(
                            &op_id,
                            OperationStatus::Failed,
                            Some(Value::String(error_message)),
                        )
                        .await;
                    return;
                }
            };

            let shell_cmd = ShellCommand {
                id: op_id.clone(),
                command: full_command,
                working_dir: wd,
                timeout_ms: timeout.map_or(
                    shell_pool.config().command_timeout.as_millis() as u64,
                    |t| t * 1000,
                ),
            };

            let result = shell.execute_command(shell_cmd).await;
            shell_pool.return_shell(shell).await;

            match result {
                Ok(output) => {
                    let final_output = json!({
                        "stdout": output.stdout,
                        "stderr": output.stderr,
                        "exit_code": output.exit_code,
                    });
                    let status = if output.exit_code == 0 {
                        OperationStatus::Completed
                    } else {
                        OperationStatus::Failed
                    };
                    monitor
                        .update_status(&op_id, status, Some(final_output))
                        .await;
                }
                Err(e) => {
                    monitor
                        .update_status(
                            &op_id,
                            OperationStatus::Failed,
                            Some(Value::String(e.to_string())),
                        )
                        .await;
                }
            }
        });

        op_id_clone
    }
}
