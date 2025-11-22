use crate::client::Client;
use anyhow::Result;
use std::time::Duration;
use tempfile::TempDir;

pub async fn setup_mcp_service_with_client() -> Result<(TempDir, Client)> {
    // Create a temporary directory for tool configs
    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path();
    let tool_config_path = tools_dir.join("long_running_async.json");

    let tool_config_content = r#"
    {
        "name": "long_running_async",
        "description": "A long running async command",
        "command": "sleep",
        "timeout_seconds": 30,
        "synchronous": false,
        "enabled": true,
        "subcommand": [
            {
                "name": "default",
                "description": "sleeps for a given duration",
                "positional_args": [
                    {
                        "name": "duration",
                        "option_type": "string",
                        "description": "duration to sleep",
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

// Common test utilities from tests/common/mod.rs

use crate::adapter::Adapter;
use crate::operation_monitor::{MonitorConfig, OperationMonitor};
use crate::shell_pool::{ShellPoolConfig, ShellPoolManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Get the workspace directory for tests
#[allow(dead_code)]
pub fn get_workspace_dir() -> PathBuf {
    // In a workspace, CARGO_MANIFEST_DIR points to the crate directory (ahma_core)
    // We need to go up one level to get to the workspace root where test-data lives
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf()
}

/// Get a path relative to the workspace root
#[allow(dead_code)]
pub fn get_workspace_path<P: AsRef<Path>>(relative: P) -> PathBuf {
    get_workspace_dir().join(relative)
}

/// Get the `.ahma/tools` directory path
#[allow(dead_code)]
pub fn get_tools_dir() -> PathBuf {
    get_workspace_path(".ahma/tools")
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

    Adapter::new(operation_monitor, shell_pool_manager).map(Arc::new)
}

/// Strip ANSI escape sequences so tests are robust across local vs CI where
/// colored output (stderr merge) may contain escape codes that break simple
/// substring assertions.
#[allow(dead_code)]
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // ESC
            if let Some('[') = chars.peek() {
                // consume '['
                chars.next();
                // consume until a terminator in @A–Z[\]^_`a–z{|}~ (0x40..=0x7E)
                while let Some(&nc) = chars.peek() {
                    let code = nc as u32;
                    if (0x40..=0x7E).contains(&code) {
                        // end of CSI sequence
                        chars.next();
                        break;
                    } else {
                        chars.next();
                    }
                }
                continue; // skip entire escape sequence
            }
            // If it's ESC but not CSI, skip just ESC
            continue;
        }
        out.push(c);
    }
    out
}

// Test client module
pub mod test_client {
    use super::{get_workspace_dir, get_workspace_path};
    use anyhow::Result;
    use rmcp::{
        ServiceExt,
        service::{RoleClient, RunningService},
        transport::{ConfigureCommandExt, TokioChildProcess},
    };
    use std::path::Path;
    use tokio::process::Command;

    #[allow(dead_code)]
    pub async fn new_client(tools_dir: Option<&str>) -> Result<RunningService<RoleClient, ()>> {
        new_client_with_args(tools_dir, &[]).await
    }

    #[allow(dead_code)]
    pub async fn new_client_with_args(
        tools_dir: Option<&str>,
        extra_args: &[&str],
    ) -> Result<RunningService<RoleClient, ()>> {
        let workspace_dir = get_workspace_dir();
        let client = ()
            .serve(TokioChildProcess::new(Command::new("cargo").configure(
                |cmd| {
                    cmd.current_dir(&workspace_dir)
                        .arg("run")
                        .arg("--package")
                        .arg("ahma_shell")
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
            .await?;
        Ok(client)
    }

    #[allow(dead_code)]
    pub async fn new_client_in_dir(
        tools_dir: Option<&str>,
        extra_args: &[&str],
        working_dir: &Path,
    ) -> Result<RunningService<RoleClient, ()>> {
        let workspace_dir = get_workspace_dir();
        let client = ()
            .serve(TokioChildProcess::new(Command::new("cargo").configure(
                |cmd| {
                    cmd.current_dir(working_dir)
                        .arg("run")
                        .arg("--manifest-path")
                        .arg(workspace_dir.join("Cargo.toml"))
                        .arg("--package")
                        .arg("ahma_shell")
                        .arg("--bin")
                        .arg("ahma_mcp")
                        .arg("--");
                    if let Some(dir) = tools_dir {
                        let tools_path = if Path::new(dir).is_absolute() {
                            Path::new(dir).to_path_buf()
                        } else {
                            // If tools_dir is relative, it should be relative to the working_dir
                            // or we should resolve it relative to workspace if that's what tests expect.
                            // Most tests pass ".ahma/tools" which is in workspace.
                            // If we change CWD, we must resolve it absolutely.
                            get_workspace_path(dir)
                        };
                        cmd.arg("--tools-dir").arg(tools_path);
                    }
                    for arg in extra_args {
                        cmd.arg(arg);
                    }
                },
            ))?)
            .await?;
        Ok(client)
    }
}

// Test project module
pub mod test_project {
    #![allow(dead_code)]

    use super::Result;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::task::spawn_blocking;

    /// Options to customize a temporary project with various tool configurations.
    #[derive(Debug, Clone, Default)]
    pub struct TestProjectOptions {
        /// Prefix for the temp dir name. A process ID will be appended automatically for uniqueness.
        pub prefix: Option<String>,
        /// Whether to create a Cargo project structure
        pub with_cargo: bool,
        /// Whether to create test files for sed operations
        pub with_text_files: bool,
        /// Whether to include tool configuration files
        pub with_tool_configs: bool,
    }

    /// Create a temporary project with flexible tool configurations for testing ahma_mcp.
    /// Ensures unique directory via tempfile and process ID and never writes to the repo root.
    pub async fn create_rust_test_project(opts: TestProjectOptions) -> Result<TempDir> {
        let process_id = std::process::id();
        let prefix = opts.prefix.unwrap_or_else(|| "ahma_mcp_test_".to_string());

        // TempDir creation is synchronous; use spawn_blocking to keep async threads unblocked under load.
        let temp_dir = spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix(&format!("{}{}_", prefix, process_id))
                .tempdir()
        })
        .await?
        .map_err(anyhow::Error::from)?;

        let project_path = temp_dir.path();

        // Create directory structure based on options
        if opts.with_cargo {
            create_cargo_structure(project_path).await?;
        }

        if opts.with_text_files {
            create_text_files(project_path).await?;
        }

        if opts.with_tool_configs {
            create_tool_configs(project_path).await?;
        }

        Ok(temp_dir)
    }

    async fn create_cargo_structure(project_path: &Path) -> Result<()> {
        fs::create_dir_all(project_path.join("src")).await?;
        fs::write(
            project_path.join("Cargo.toml"),
            r#"
[package]
name = "test_project"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
"#,
        )
        .await?;
        fs::write(
            project_path.join("src/main.rs"),
            r#"
#[tokio::main]
async fn main() {
    println!("Hello, world!");
}
"#,
        )
        .await?;
        Ok(())
    }

    async fn create_text_files(project_path: &Path) -> Result<()> {
        fs::write(project_path.join("test1.txt"), "line1\nline2\nline3\n").await?;
        fs::write(project_path.join("test2.txt"), "foo\nbar\nbaz\n").await?;
        Ok(())
    }

    async fn create_tool_configs(project_path: &Path) -> Result<()> {
        let tools_dir = project_path.join(".ahma").join("tools");
        fs::create_dir_all(&tools_dir).await?;
        fs::write(
            tools_dir.join("echo.json"),
            r#"
{
    "name": "echo",
    "description": "Echo a message",
    "command": "echo",
    "timeout_seconds": 10,
    "synchronous": true,
    "enabled": true,
    "subcommand": [
        {
            "name": "default",
            "description": "echo the message",
            "positional_args": [
                {
                    "name": "message",
                    "option_type": "string",
                    "description": "message to echo",
                    "required": true
                }
            ]
        }
    ]
}
"#,
        )
        .await?;
        Ok(())
    }

    /// Create a temporary project with full Rust project setup for testing
    pub async fn create_full_rust_test_project() -> Result<TempDir> {
        create_rust_test_project(TestProjectOptions {
            prefix: None,
            with_cargo: true,
            with_text_files: true,
            with_tool_configs: true,
        })
        .await
    }
}

#[allow(clippy::module_inception, dead_code)]
pub mod test_utils {
    use crate::{
        adapter::Adapter,
        client::Client,
        mcp_service::AhmaMcpService,
        operation_monitor::{MonitorConfig, OperationMonitor},
        shell_pool::{ShellPoolConfig, ShellPoolManager},
    };
    use anyhow::Result;
    use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
    use tempfile::{TempDir, tempdir};

    pub fn init_test_logging() {
        let _ = crate::utils::logging::init_logging("trace", false);
    }

    /// Check if output contains any of the expected patterns
    #[allow(dead_code)]
    pub fn contains_any(output: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|pattern| output.contains(pattern))
    }

    /// Check if output contains all of the expected patterns
    pub fn contains_all(output: &str, patterns: &[&str]) -> bool {
        patterns.iter().all(|pattern| output.contains(pattern))
    }

    /// Extract tool schemas from debug output
    pub fn extract_tool_names(debug_output: &str) -> Vec<String> {
        let mut tool_names = Vec::new();
        for line in debug_output.lines() {
            if (line.contains("Loading tool:") || line.contains("Tool loaded:"))
                && let Some(name) = line.split(':').nth(1)
            {
                tool_names.push(name.trim().to_string());
            }
        }
        tool_names
    }

    /// Verify that a path exists and is a file
    pub async fn file_exists(path: &Path) -> bool {
        tokio::fs::metadata(path)
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    /// Verify that a path exists and is a directory
    pub async fn dir_exists(path: &Path) -> bool {
        tokio::fs::metadata(path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    /// Create a temporary directory with tool configs for testing
    pub async fn create_temp_tools_dir() -> super::Result<(TempDir, Client)> {
        let temp_dir = tempdir()?;
        let tools_dir = temp_dir.path().join("tools");
        tokio::fs::create_dir_all(&tools_dir).await?;

        let mut client = Client::new();
        client
            .start_process(Some(tools_dir.to_str().unwrap()))
            .await?;

        Ok((temp_dir, client))
    }

    /// Read a file and return its contents as a string
    pub async fn read_file_contents(path: &Path) -> Result<String> {
        Ok(tokio::fs::read_to_string(path).await?)
    }

    /// Write contents to a file
    pub async fn write_file_contents(path: &Path, contents: &str) -> Result<()> {
        Ok(tokio::fs::write(path, contents).await?)
    }

    use tokio::sync::mpsc::{Receiver, Sender};

    pub async fn setup_test_environment() -> (AhmaMcpService, TempDir) {
        let temp_dir = tempdir().unwrap();
        let tools_dir = temp_dir.path().join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();

        let monitor_config = MonitorConfig::with_timeout(Duration::from_secs(30));
        let monitor = Arc::new(OperationMonitor::new(monitor_config));
        let shell_pool_config = ShellPoolConfig::default();
        let shell_pool = Arc::new(ShellPoolManager::new(shell_pool_config));
        let adapter = Arc::new(Adapter::new(monitor.clone(), shell_pool).unwrap());

        // Create empty configs and guidance for the new API
        let configs = Arc::new(HashMap::new());
        let guidance = Arc::new(None);

        let service = AhmaMcpService::new(adapter, monitor, configs, guidance, false)
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
        let adapter = Arc::new(Adapter::new(monitor.clone(), shell_pool).unwrap());

        // Create empty configs and guidance for the new API
        let configs = Arc::new(HashMap::new());
        let guidance = Arc::new(None);

        let service = AhmaMcpService::new(adapter, monitor, configs, guidance, false)
            .await
            .unwrap();

        let (input_tx, output_rx) = tokio::sync::mpsc::channel(100);
        (service, input_tx, output_rx, temp_dir)
    }

    /// Setup MCP service with a test client for integration testing
    pub async fn setup_mcp_service_with_client() -> Result<(TempDir, Client)> {
        let temp_dir = tempdir()?;
        let tools_dir = temp_dir.path().join("tools");
        tokio::fs::create_dir_all(&tools_dir).await?;

        let mut client = Client::new();
        client
            .start_process(Some(tools_dir.to_str().unwrap()))
            .await?;

        Ok((temp_dir, client))
    }

    /// Helper to assert that formatting a JSON string via TerminalOutput contains all expected substrings.
    /// Falls back to raw string if parsing fails (mirroring format_content behavior).
    #[allow(dead_code)]
    pub fn assert_formatted_json_contains(raw: &str, expected: &[&str]) {
        let formatted = crate::terminal_output::TerminalOutput::format_content(raw);
        for token in expected {
            assert!(
                formatted.contains(token),
                "Formatted content missing token: {token}"
            );
        }
    }
}
