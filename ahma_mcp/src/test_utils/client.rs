use super::fs::get_workspace_dir;
pub use super::fs::get_workspace_tools_dir;

use crate::adapter::Adapter;
use crate::client::Client;
use crate::mcp_service::AhmaMcpService;
use crate::operation_monitor::{MonitorConfig, OperationMonitor};
use crate::shell_pool::{ShellPoolConfig, ShellPoolManager};
use anyhow::{Context, Result};
use rmcp::{
    ServiceExt,
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tempfile::{TempDir, tempdir};
use tokio::process::Command;
use tokio::sync::mpsc::{Receiver, Sender};

/// Cached path to the pre-built ahma_mcp binary.
static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Get the path to the ahma_mcp binary.
fn get_test_binary_path() -> PathBuf {
    BINARY_PATH
        .get_or_init(|| {
            // Check env var first
            if let Ok(path) = std::env::var("AHMA_TEST_BINARY") {
                let p = PathBuf::from(&path);
                if p.exists() {
                    return p;
                }
                eprintln!(
                    "Warning: AHMA_TEST_BINARY={} does not exist, falling back",
                    path
                );
            }

            // Check for debug binary
            let workspace = get_workspace_dir();

            // Check CARGO_TARGET_DIR
            if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
                let p = PathBuf::from(target_dir).join("debug/ahma_mcp");
                if p.exists() {
                    return p;
                }
            }

            let debug_binary = workspace.join("target/debug/ahma_mcp");
            if debug_binary.exists() {
                return debug_binary;
            }

            // Check for release binary
            let release_binary = workspace.join("target/release/ahma_mcp");
            if release_binary.exists() {
                return release_binary;
            }

            // No pre-built binary found - return empty path to signal fallback
            PathBuf::new()
        })
        .clone()
}

fn use_prebuilt_binary() -> bool {
    let path = get_test_binary_path();
    !path.as_os_str().is_empty() && path.exists()
}

/// Builder for creating MCP clients in tests.
#[derive(Default)]
pub struct ClientBuilder {
    tools_dir: Option<PathBuf>,
    extra_args: Vec<String>,
    extra_env: Vec<(String, String)>,
    working_dir: Option<PathBuf>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tools_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        let path = path.as_ref();
        if path.is_absolute() {
            self.tools_dir = Some(path.to_path_buf());
        } else {
            // Resolve relative to workspace or working dir if set?
            // Existing logic resolved relative to workspace if not absolute.
            // Let's resolve it here to avoid ambiguity.
            // If working_dir is set later, it might be tricky.
            // For now, let's keep the existing logic: if relative, resolve via get_workspace_path
            // BUT, `new_client_in_dir` resolved relative to `working_dir`.
            // So we'll store it as is and resolve at build time.
            self.tools_dir = Some(path.to_path_buf());
        }
        self
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for arg in args {
            self.extra_args.push(arg.into());
        }
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    pub fn working_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.working_dir = Some(path.as_ref().to_path_buf());
        self
    }

    pub async fn build(self) -> Result<RunningService<RoleClient, ()>> {
        let workspace_dir = get_workspace_dir();
        let working_dir = self
            .working_dir
            .clone()
            .unwrap_or_else(|| workspace_dir.clone());

        if use_prebuilt_binary() {
            let binary_path = get_test_binary_path();
            self.run_command(Command::new(&binary_path), &working_dir)
                .await
        } else {
            eprintln!(
                "Warning: Using slow 'cargo run' path. Run 'cargo build' first for faster tests."
            );
            let mut cmd = Command::new("cargo");
            cmd.arg("run")
                .arg("--manifest-path")
                .arg(workspace_dir.join("Cargo.toml"))
                .arg("--package")
                .arg("ahma_mcp")
                .arg("--bin")
                .arg("ahma_mcp")
                .arg("--");
            self.run_command(cmd, &working_dir).await
        }
    }

    async fn run_command(
        self,
        command: Command,
        working_dir: &Path,
    ) -> Result<RunningService<RoleClient, ()>> {
        ().serve(TokioChildProcess::new(command.configure(|cmd| {
            cmd.env("AHMA_TEST_MODE", "1")
                .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                .current_dir(working_dir)
                .kill_on_drop(true);

            for (k, v) in self.extra_env {
                cmd.env(k, v);
            }

            if let Some(dir) = self.tools_dir {
                let tools_path = if dir.is_absolute() {
                    dir
                } else {
                    // Resolve relative to working_dir
                    // Note: Original code logic was:
                    // new_client uses get_workspace_path(dir)
                    // new_client_in_dir uses working_dir.join(dir)
                    // Since default working_dir is workspace, this unifies correctly.
                    working_dir.join(dir)
                };
                cmd.arg("--tools-dir").arg(tools_path);
            }
            for arg in self.extra_args {
                cmd.arg(arg);
            }
        }))?)
        .await
        .context("Failed to start client service")
    }
}

// Backward compatibility wrappers

pub async fn setup_mcp_service_with_client() -> Result<(TempDir, Client)> {
    // Create a temporary directory for tool configs
    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path();
    let tool_config_path = tools_dir.join("sandboxed_shell.json");

    let tool_config_content = r#"
    {
        "name": "sandboxed_shell",
        "description": "Execute shell commands asynchronously",
        "command": "bash -c",
        "timeout_seconds": 30,
        "synchronous": false,
        "enabled": true,
        "subcommand": [
            {
                "name": "default",
                "description": "Execute a shell command asynchronously",
                "positional_args": [
                    {
                        "name": "command",
                        "type": "string",
                        "description": "shell command to execute",
                        "required": true
                    }
                ]
            }
        ]
    }
    "#;
    std::fs::write(&tool_config_path, tool_config_content)?;

    let mut client = Client::new();
    client
        .start_process(Some(tools_dir.to_str().unwrap()))
        .await?;

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(
        crate::constants::SEQUENCE_STEP_DELAY_MS,
    ))
    .await;

    Ok((temp_dir, client))
}

pub async fn setup_test_environment() -> (AhmaMcpService, TempDir) {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();

    let config = super::config::default_config();
    let monitor_config = MonitorConfig::with_timeout(config.default_timeout);
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
    let sandbox = Arc::new(crate::sandbox::Sandbox::new_test());
    let adapter = Arc::new(Adapter::new(monitor.clone(), shell_pool, sandbox).unwrap());

    // Create empty configs and guidance for the new API
    let configs = Arc::new(HashMap::new());
    let guidance = Arc::new(None);

    let service = AhmaMcpService::new(adapter, monitor, configs, guidance, false, false)
        .await
        .unwrap();

    (service, temp_dir)
}

#[allow(dead_code)]
pub async fn setup_test_environment_with_io()
-> (AhmaMcpService, Sender<String>, Receiver<String>, TempDir) {
    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    // Use Tokio's async filesystem API so we don't block the runtime
    tokio::fs::create_dir_all(&tools_dir).await.unwrap();

    let config = super::config::default_config();
    let monitor_config = MonitorConfig::with_timeout(config.default_timeout);
    let monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig::default();
    let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
    let sandbox = Arc::new(crate::sandbox::Sandbox::new_test());
    let adapter = Arc::new(Adapter::new(monitor.clone(), shell_pool, sandbox).unwrap());

    // Create empty configs and guidance for the new API
    let configs = Arc::new(HashMap::new());
    let guidance = Arc::new(None);

    let service = AhmaMcpService::new(adapter, monitor, configs, guidance, false, false)
        .await
        .unwrap();

    let (input_tx, output_rx) = tokio::sync::mpsc::channel(100);
    (service, input_tx, output_rx, temp_dir)
}

/// Create a test config for integration tests
#[allow(dead_code)]
pub fn create_test_config(_workspace_dir: &Path) -> Result<Arc<Adapter>> {
    let config = super::config::default_config();
    // Create test monitor and shell pool configurations
    let monitor_config = MonitorConfig::with_timeout(config.default_timeout);
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    let shell_pool_config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: config.max_concurrent_tasks as usize,
        shell_idle_timeout: Duration::from_secs(1800),
        pool_cleanup_interval: Duration::from_secs(300),
        shell_spawn_timeout: config.quick_timeout,
        command_timeout: config.default_timeout,
        health_check_interval: Duration::from_secs(60),
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));

    // Create a test sandbox
    let sandbox = Arc::new(crate::sandbox::Sandbox::new_test());

    Adapter::new(operation_monitor, shell_pool_manager, sandbox).map(Arc::new)
}
