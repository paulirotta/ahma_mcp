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

use anyhow::{Context, Result};
use rmcp::model::{CallToolResult, Content};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::shell_pool::{ShellCommand, ShellPoolConfig, ShellPoolManager};

/// Defines whether a command should run synchronously (blocking) or asynchronously (non-blocking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Synchronous,
    Asynchronous,
}

/// The main adapter that handles command execution.
#[derive(Debug)]
pub struct Adapter {
    /// Whether to force all operations to run synchronously.
    synchronous_mode: bool,
    /// Default command timeout in seconds.
    timeout_secs: u64,
    /// Pre-warmed shell pool manager for async execution.
    shell_pool: Arc<ShellPoolManager>,
}

impl Adapter {
    /// Create a new adapter with a specific timeout.
    pub fn with_timeout(synchronous_mode: bool, timeout_secs: u64) -> Result<Self> {
        let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));
        shell_pool.clone().start_background_tasks();

        Ok(Adapter {
            synchronous_mode,
            timeout_secs,
            shell_pool,
        })
    }

    /// Execute a tool with the given arguments in an optional working directory.
    ///
    /// # Arguments
    /// * `command` - The base command to execute (e.g., "cargo").
    /// * `args` - A vector of string arguments for the command (e.g., ["build", "--release"]).
    /// * `working_directory` - The absolute path to the directory where the command should run.
    /// * `mode` - The execution mode (sync or async).
    /// * `hints` - Optional hints to return for async operations.
    pub async fn execute_tool_in_dir(
        &self,
        command: &str,
        args: Vec<String>,
        working_directory: Option<String>,
        mode: ExecutionMode,
        hints: Option<HashMap<String, String>>,
    ) -> Result<CallToolResult> {
        let mut full_cmd_args = vec![command.to_string()];
        full_cmd_args.extend(args.clone());

        // Global synchronous flag overrides everything
        if self.synchronous_mode || mode == ExecutionMode::Synchronous {
            let result = self
                .execute_sync_in_dir(&full_cmd_args, working_directory.as_deref())
                .await?;
            Ok(CallToolResult::success(vec![Content::text(result)]))
        } else {
            self.execute_async_in_dir(command, &args, working_directory.as_deref(), hints)
                .await
        }
    }

    /// Execute command synchronously with optional working directory.
    async fn execute_sync_in_dir(
        &self,
        args: &[String],
        working_directory: Option<&str>,
    ) -> Result<String> {
        let mut cmd = Command::new(&args[0]);
        if args.len() > 1 {
            cmd.args(&args[1..]);
        }

        if let Some(dir) = working_directory {
            cmd.current_dir(dir);
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await
        .context("Command timed out")?
        .context("Failed to execute command")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("Command failed: {}", stderr))
        }
    }

    /// Spawns a command asynchronously using the shell pool and immediately returns an operation ID.
    async fn execute_async_in_dir(
        &self,
        command: &str,
        args: &[String],
        working_directory: Option<&str>,
        hints: Option<HashMap<String, String>>,
    ) -> Result<CallToolResult> {
        let op_id = Uuid::new_v4().to_string();

        // If we have a working directory and shell pooling is enabled, use it.
        if let Some(dir) = working_directory {
            if let Some(mut shell) = self.shell_pool.get_shell(dir).await {
                let shell_cmd = ShellCommand {
                    id: op_id.clone(),
                    command: [command]
                        .iter()
                        .map(|s| s.to_string())
                        .chain(args.iter().cloned())
                        .collect(),
                    working_dir: dir.to_string(),
                    timeout_ms: self.timeout_secs * 1000,
                };

                // Execute in the background, don't await the final result here.
                let _shell_pool = self.shell_pool.clone();
                tokio::spawn(async move {
                    info!(
                        "Starting async shell operation (op_id: {}): {:?}",
                        shell_cmd.id, shell_cmd.command
                    );
                    match shell.execute_command(shell_cmd).await {
                        Ok(resp) if resp.exit_code == 0 => {
                            info!("Async shell op {} succeeded.", resp.id);
                            // TODO: Store result
                        }
                        Ok(resp) => {
                            warn!(
                                "Async shell op {} failed with code {}: {}",
                                resp.id, resp.exit_code, resp.stderr
                            );
                            // TODO: Store result
                        }
                        Err(e) => {
                            error!("Async shell op failed to execute: {}", e);
                            // TODO: Store result
                        }
                    }
                    _shell_pool.return_shell(shell).await;
                });
            } else {
                // Fallback to spawning a regular command if shell is not available
                self.spawn_background_command(&op_id, command, args, working_directory)
                    .await;
            }
        } else {
            // Fallback for commands without a working directory
            self.spawn_background_command(&op_id, command, args, working_directory)
                .await;
        }

        // Immediately return the operation ID and hints to the client
        let mut content = vec![Content::text(format!("operation_id: {}", op_id))];
        if let Some(hints_map) = hints {
            let subcommand_name = args.first().map(|s| s.as_str()).unwrap_or("");
            let hint_text = hints_map
                .get(subcommand_name)
                .or_else(|| hints_map.get("default"))
                .map(|s| s.as_str());

            if let Some(text) = hint_text {
                content.push(Content::text(text));
            }
        }

        Ok(CallToolResult::success(content))
    }

    /// Helper to spawn a standard tokio::process::Command in the background.
    async fn spawn_background_command(
        &self,
        op_id: &str,
        command: &str,
        args: &[String],
        working_directory: Option<&str>,
    ) {
        let op_id = op_id.to_string();
        let command = command.to_string();
        let args = args.to_vec();
        let wd = working_directory.map(|s| s.to_string());
        let timeout_secs = self.timeout_secs;

        tokio::spawn(async move {
            info!(
                "Starting async operation (op_id: {}): {} {:?}",
                op_id, command, args
            );
            let mut cmd = Command::new(&command);
            cmd.args(&args);
            if let Some(dir) = wd {
                cmd.current_dir(dir);
            }

            let result =
                tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
                    .await;

            // TODO: Store the result somewhere for the client to fetch using the op_id.
            match result {
                Ok(Ok(output)) => {
                    if output.status.success() {
                        info!("Async op {} finished successfully.", op_id);
                    } else {
                        warn!(
                            "Async op {} failed. Stderr: {}",
                            op_id,
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
                Ok(Err(e)) => error!("Async op {} failed to execute: {}", op_id, e),
                Err(_) => warn!("Async op {} timed out.", op_id),
            }
        });
    }
}
