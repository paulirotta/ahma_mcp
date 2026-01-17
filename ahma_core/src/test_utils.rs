use crate::client::Client;
use crate::sandbox;
use anyhow::Result;
use std::sync::Once;
use std::time::Duration;
use tempfile::TempDir;

// =============================================================================
// Skip-if-disabled macros for test utilities
// =============================================================================

/// Check if a tool is disabled in the config or environment.
/// Returns true if:
/// 1. Environment variable `AHMA_DISABLE_TOOL_{TOOL_NAME_UPPER}` is "true" or "1"
/// 2. The tool JSON exists in `.ahma/` and has `"enabled": false`
/// 3. The tool JSON exists in `ahma_core/examples/configs/` and has `"enabled": false`
pub fn is_tool_disabled(tool_name: &str) -> bool {
    // Check environment variable first (e.g., AHMA_DISABLE_TOOL_GH=true)
    let env_var = format!("AHMA_DISABLE_TOOL_{}", tool_name.to_uppercase());
    if std::env::var(&env_var)
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false)
    {
        return true;
    }

    let workspace_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf();

    // Paths to check for tool configuration
    let config_paths = [
        workspace_dir
            .join(".ahma")
            .join(format!("{}.json", tool_name)),
        workspace_dir
            .join("ahma_core/examples/configs")
            .join(format!("{}.json", tool_name)),
    ];

    for config_path in config_paths {
        if config_path.exists()
            && let Ok(content) = std::fs::read_to_string(&config_path)
        {
            // Simple check for "enabled": false
            if content.contains(r#""enabled": false"#) || content.contains(r#""enabled":false"#) {
                return true;
            }
        }
    }

    false
}

/// Macro to skip a synchronous test if a tool is disabled.
///
/// A tool is considered disabled if a configuration file named `{tool_name}.json`
/// exists in the `.ahma/` directory at the project root and contains `"enabled": false`.
/// If the file does not exist, the tool is assumed to be enabled.
///
/// This is used to skip tests that require external tools (like `gh` or `python`)
/// when they are not available or should not be used in the current environment.
///
/// # Example of a disabled tool definition in `.ahma/tool_name.json`:
/// ```json
/// {
///   "enabled": false
/// }
/// ```
///
/// Macro to skip a synchronous test if a tool is disabled.
///
/// This checks if a tool is disabled in the project configuration (e.g., in `.ahma/tool_name.json`)
/// and returns early from the current function if it is. This is useful for avoiding
/// test failures when optional tools are disabled in the environment.
///
/// # Example
///
/// ```rust
/// fn test_my_tool_sync() {
///     skip_if_disabled!("my_tool");
///     // ... test implementation
/// }
/// ```
#[macro_export]
macro_rules! skip_if_disabled {
    ($tool_name:expr) => {
        if $crate::test_utils::is_tool_disabled($tool_name) {
            eprintln!("⚠️  Skipping test - {} is disabled in config", $tool_name);
            return;
        }
    };
}

/// Macro to skip an async test that returns Result if a tool is disabled.
///
/// This is used to prevent integration tests from running when their required tools
/// are explicitly disabled in the project configuration. This variant returns `Ok(())`
/// to satisfy tests that return a `Result<_, _>`.
///
/// # Example
///
/// ```rust
/// #[tokio::test]
/// async fn test_my_tool() -> anyhow::Result<()> {
///     skip_if_disabled_async_result!("my_tool");
///     // ... test implementation
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! skip_if_disabled_async_result {
    ($tool_name:expr) => {
        if $crate::test_utils::is_tool_disabled($tool_name) {
            eprintln!("⚠️  Skipping test - {} is disabled in config", $tool_name);
            return Ok(());
        }
    };
}

/// Macro to skip an async test (no return value) if a tool is disabled.
///
/// This is used to prevent integration tests from running when their required tools
/// are explicitly disabled in the project configuration. This is particularly useful
/// for avoiding failures in environments where certain heavy or external tools are
/// not available or should be ignored.
///
/// # Example
///
/// ```rust
/// #[tokio::test]
/// async fn test_my_tool() {
///     skip_if_disabled_async!("my_tool");
///     // ... test implementation
/// }
/// ```
#[macro_export]
macro_rules! skip_if_disabled_async {
    ($tool_name:expr) => {
        if $crate::test_utils::is_tool_disabled($tool_name) {
            eprintln!("⚠️  Skipping test - {} is disabled in config", $tool_name);
            return;
        }
    };
}

/// Initialize sandbox for tests. Uses "/" as the sandbox scope to allow all operations.
/// This is safe because tests run in controlled environments.
/// This function is idempotent - calling it multiple times is safe.
static SANDBOX_INIT: Once = Once::new();

pub fn init_test_sandbox() {
    SANDBOX_INIT.call_once(|| {
        // Enable test mode to bypass sandbox requirement
        sandbox::enable_test_mode();

        // Initialize with root "/" to allow all paths in tests
        // This is safe because:
        // 1. Tests run in controlled environments
        // 2. Tests don't execute untrusted AI commands
        // 3. We need flexibility to test various path scenarios
        if let Err(e) = sandbox::initialize_sandbox_scope(std::path::Path::new("/")) {
            // If already initialized (from another test), that's fine
            tracing::debug!("Sandbox initialization in test: {:?}", e);
        }
    });
}

pub async fn setup_mcp_service_with_client() -> Result<(TempDir, Client)> {
    // Ensure sandbox is initialized for tests
    init_test_sandbox();

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

/// Get the `.ahma` directory path
#[allow(dead_code)]
pub fn get_tools_dir() -> PathBuf {
    get_workspace_path(".ahma")
}

/// Create a test config for integration tests
#[allow(dead_code)]
pub fn create_test_config(_workspace_dir: &Path) -> Result<Arc<Adapter>> {
    // Ensure sandbox is initialized for tests
    init_test_sandbox();

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
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;
    use tokio::process::Command;

    /// Cached path to the pre-built ahma_mcp binary.
    /// Using a pre-built binary instead of `cargo run` speeds up tests by 5-10x.
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();

    /// Get the path to the ahma_mcp binary.
    ///
    /// Priority:
    /// 1. AHMA_TEST_BINARY env var (for CI or custom builds)
    /// 2. target/debug/ahma_mcp (if exists)
    /// 3. Returns empty path to signal fallback to cargo run
    fn get_binary_path() -> PathBuf {
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

    /// Returns true if we should use the pre-built binary (fast path)
    fn use_prebuilt_binary() -> bool {
        let path = get_binary_path();
        !path.as_os_str().is_empty() && path.exists()
    }

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

        let client = if use_prebuilt_binary() {
            // Fast path: use pre-built binary directly
            let binary_path = get_binary_path();
            ().serve(TokioChildProcess::new(
                Command::new(&binary_path).configure(|cmd| {
                    cmd.env("AHMA_TEST_MODE", "1")
                        // Keep tests deterministic even if the developer/CI environment sets these.
                        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                        .current_dir(&workspace_dir);
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
            // Slow fallback: use cargo run (for when binary isn't built)
            eprintln!(
                "Warning: Using slow 'cargo run' path. Run 'cargo build' first for faster tests."
            );
            ().serve(TokioChildProcess::new(Command::new("cargo").configure(
                |cmd| {
                    cmd.env("AHMA_TEST_MODE", "1")
                        // Keep tests deterministic even if the developer/CI environment sets these.
                        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                        .current_dir(&workspace_dir)
                        .arg("run")
                        .arg("--package")
                        .arg("ahma_core")
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

    #[allow(dead_code)]
    pub async fn new_client_in_dir(
        tools_dir: Option<&str>,
        extra_args: &[&str],
        working_dir: &Path,
    ) -> Result<RunningService<RoleClient, ()>> {
        let workspace_dir = get_workspace_dir();

        let client = if use_prebuilt_binary() {
            // Fast path: use pre-built binary directly
            let binary_path = get_binary_path();
            ().serve(TokioChildProcess::new(
                Command::new(&binary_path).configure(|cmd| {
                    cmd.env("AHMA_TEST_MODE", "1")
                        // Keep tests deterministic even if the developer/CI environment sets these.
                        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                        .current_dir(working_dir);
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
            // Slow fallback: use cargo run
            eprintln!(
                "Warning: Using slow 'cargo run' path. Run 'cargo build' first for faster tests."
            );
            ().serve(TokioChildProcess::new(Command::new("cargo").configure(
                |cmd| {
                    cmd.env("AHMA_TEST_MODE", "1")
                        // Keep tests deterministic even if the developer/CI environment sets these.
                        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                        .current_dir(working_dir)
                        .arg("run")
                        .arg("--manifest-path")
                        .arg(workspace_dir.join("Cargo.toml"))
                        .arg("--package")
                        .arg("ahma_core")
                        .arg("--bin")
                        .arg("ahma_mcp")
                        .arg("--");
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

    #[allow(dead_code)]
    pub async fn new_client_in_dir_with_env(
        tools_dir: Option<&str>,
        extra_args: &[&str],
        working_dir: &Path,
        extra_env: &[(&str, &str)],
    ) -> Result<RunningService<RoleClient, ()>> {
        let workspace_dir = get_workspace_dir();

        let client = if use_prebuilt_binary() {
            let binary_path = get_binary_path();
            ().serve(TokioChildProcess::new(
                Command::new(&binary_path).configure(|cmd| {
                    cmd.env("AHMA_TEST_MODE", "1")
                        // Keep tests deterministic even if the developer/CI environment sets these.
                        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                        .current_dir(working_dir);

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
                        // Keep tests deterministic even if the developer/CI environment sets these.
                        .env_remove("AHMA_SKIP_SEQUENCE_TOOLS")
                        .env_remove("AHMA_SKIP_SEQUENCE_SUBCOMMANDS")
                        .current_dir(working_dir)
                        .arg("run")
                        .arg("--manifest-path")
                        .arg(workspace_dir.join("Cargo.toml"))
                        .arg("--package")
                        .arg("ahma_core")
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

    /// Get the absolute path to the workspace tools directory
    #[allow(dead_code)]
    pub fn get_workspace_tools_dir() -> std::path::PathBuf {
        get_workspace_path(".ahma")
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
        let tools_dir = project_path.join(".ahma");
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
        let adapter = Arc::new(Adapter::new(monitor.clone(), shell_pool).unwrap());

        // Create empty configs and guidance for the new API
        let configs = Arc::new(HashMap::new());
        let guidance = Arc::new(None);

        let service = AhmaMcpService::new(adapter, monitor, configs, guidance, false, false)
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
