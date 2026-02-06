pub use super::file_helpers::get_workspace_tools_dir;
use super::file_helpers::{get_workspace_dir, get_workspace_path};

use crate::adapter::Adapter;
use crate::client::Client;
use crate::mcp_service::AhmaMcpService;
use crate::operation_monitor::{MonitorConfig, OperationMonitor};
use crate::shell_pool::{ShellPoolConfig, ShellPoolManager};
use anyhow::Result;
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

pub async fn new_client(tools_dir: Option<&str>) -> Result<RunningService<RoleClient, ()>> {
    new_client_with_args(tools_dir, &[]).await
}

pub async fn new_client_with_args(
    tools_dir: Option<&str>,
    extra_args: &[&str],
) -> Result<RunningService<RoleClient, ()>> {
    let workspace_dir = get_workspace_dir();

    let client = if use_prebuilt_binary() {
        // Fast path: use pre-built binary directly
        let binary_path = get_test_binary_path();
        ().serve(TokioChildProcess::new(
            Command::new(&binary_path).configure(|cmd| {
                cmd.env("AHMA_TEST_MODE", "1")
                    .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                    .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                    .current_dir(&workspace_dir)
                    .kill_on_drop(true);
                if let Some(dir) = tools_dir {
                    let tools_path = if Path::new(dir).is_absolute() {
                        Path::new(dir).to_path_buf()
                    } else {
                        get_workspace_path(dir)
                    };
                    cmd.arg("--tools-dir").arg(tools_path);
                }
                for arg in extra_args {
                    cmd.arg(arg);
                }
            }),
        )?)
        .await?
    } else {
        // Slow fallback: use cargo run
        eprintln!(
            "Warning: Using slow 'cargo run' path. Run 'cargo build' first for faster tests."
        );
        ().serve(TokioChildProcess::new(Command::new("cargo").configure(
            |cmd| {
                cmd.env("AHMA_TEST_MODE", "1")
                    .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                    .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                    .current_dir(&workspace_dir)
                    .arg("run")
                    .arg("--package")
                    .arg("ahma_mcp")
                    .arg("--bin")
                    .arg("ahma_mcp")
                    .arg("--");
                if let Some(dir) = tools_dir {
                    let tools_path = if Path::new(dir).is_absolute() {
                        Path::new(dir).to_path_buf()
                    } else {
                        get_workspace_path(dir)
                    };
                    cmd.arg("--tools-dir").arg(tools_path);
                }
                for arg in extra_args {
                    cmd.arg(arg);
                }
            },
        ))?)
        .await?
    };

    Ok(client)
}

pub async fn new_client_in_dir(
    tools_dir: Option<&str>,
    extra_args: &[&str],
    working_dir: &Path,
) -> Result<RunningService<RoleClient, ()>> {
    new_client_in_dir_with_env(tools_dir, extra_args, working_dir, &[]).await
}

pub async fn new_client_in_dir_with_env(
    tools_dir: Option<&str>,
    extra_args: &[&str],
    working_dir: &Path,
    extra_env: &[(&str, &str)],
) -> Result<RunningService<RoleClient, ()>> {
    let workspace_dir = get_workspace_dir();

    let client = if use_prebuilt_binary() {
        let binary_path = get_test_binary_path();
        ().serve(TokioChildProcess::new(
            Command::new(&binary_path).configure(|cmd| {
                cmd.env("AHMA_TEST_MODE", "1")
                    .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                    .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                    .current_dir(working_dir)
                    .kill_on_drop(true);

                for (k, v) in extra_env {
                    cmd.env(k, v);
                }

                if let Some(dir) = tools_dir {
                    let tools_path = if Path::new(dir).is_absolute() {
                        Path::new(dir).to_path_buf()
                    } else {
                        working_dir.join(dir)
                    };
                    cmd.arg("--tools-dir").arg(tools_path);
                }
                for arg in extra_args {
                    cmd.arg(arg);
                }
            }),
        )?)
        .await?
    } else {
        eprintln!(
            "Warning: Using slow 'cargo run' path. Run 'cargo build' first for faster tests."
        );
        ().serve(TokioChildProcess::new(Command::new("cargo").configure(
            |cmd| {
                cmd.env("AHMA_TEST_MODE", "1")
                    .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                    .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                    .current_dir(working_dir)
                    .arg("run")
                    .arg("--manifest-path")
                    .arg(workspace_dir.join("Cargo.toml"))
                    .arg("--package")
                    .arg("ahma_mcp")
                    .arg("--bin")
                    .arg("ahma_mcp")
                    .arg("--");

                for (k, v) in extra_env {
                    cmd.env(k, v);
                }

                if let Some(dir) = tools_dir {
                    let tools_path = if Path::new(dir).is_absolute() {
                        Path::new(dir).to_path_buf()
                    } else {
                        working_dir.join(dir)
                    };
                    cmd.arg("--tools-dir").arg(tools_path);
                }
                for arg in extra_args {
                    cmd.arg(arg);
                }
            },
        ))?)
        .await?
    };

    Ok(client)
}

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

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
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

    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
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
    // Create test monitor and shell pool configurations
    let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));

    let shell_pool_config = ShellPoolConfig {
        enabled: true,
        shells_per_directory: 2,
        max_total_shells: 20,
        shell_idle_timeout: Duration::from_secs(1800),
        pool_cleanup_interval: Duration::from_secs(300),
        shell_spawn_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(30),
        health_check_interval: Duration::from_secs(60),
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));

    // Create a test sandbox
    let sandbox = Arc::new(crate::sandbox::Sandbox::new_test());

    Adapter::new(operation_monitor, shell_pool_manager, sandbox).map(Arc::new)
}
