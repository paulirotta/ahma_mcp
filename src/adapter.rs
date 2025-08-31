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
use std::sync::Arc;
use tokio::process::Command;
use uuid::Uuid;

use crate::shell_pool::{ShellCommand, ShellPoolConfig, ShellPoolManager};

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
    pub async fn execute_tool_in_dir(
        &self,
        command: &str,
        args: Vec<String>,
        working_directory: Option<String>,
    ) -> Result<String> {
        let mut full_cmd_args = vec![command.to_string()];
        full_cmd_args.extend(args);

        if self.synchronous_mode {
            self.execute_sync_in_dir(&full_cmd_args, working_directory.as_deref())
                .await
        } else {
            self.execute_async_in_dir(&full_cmd_args, working_directory.as_deref())
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

    /// Execute command asynchronously using a pre-warmed shell when possible.
    async fn execute_async_in_dir(
        &self,
        args: &[String],
        working_directory: Option<&str>,
    ) -> Result<String> {
        // If we have a working directory and shell pooling is enabled, use it.
        if let Some(dir) = working_directory
            && let Some(mut shell) = self.shell_pool.get_shell(dir).await
        {
            let cmd = ShellCommand {
                id: Uuid::new_v4().to_string(),
                command: args.to_vec(),
                working_dir: dir.to_string(),
                timeout_ms: self.timeout_secs * 1000,
            };

            let resp = shell.execute_command(cmd).await;
            // Return shell to pool regardless of outcome.
            self.shell_pool.return_shell(shell).await;

            return match resp {
                Ok(r) if r.exit_code == 0 => Ok(r.stdout),
                Ok(r) => Err(anyhow::anyhow!(
                    "Command failed (code {}): {}",
                    r.exit_code,
                    r.stderr
                )),
                Err(e) => Err(anyhow::anyhow!("Shell execution error: {}", e)),
            };
        }

        // Fallback to sync execution if async is not possible (e.g., no working_dir).
        self.execute_sync_in_dir(args, working_directory).await
    }
}
