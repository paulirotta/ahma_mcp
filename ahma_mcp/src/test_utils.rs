//! Test helper utilities for Ahma MCP.
//!
//! This module provides reusable helpers for integration and unit tests,
//! including sandbox setup, temporary project scaffolding, and MCP client
//! conveniences. These APIs are intended for test-only code paths.

use crate::client::Client;

use anyhow::{Context, Result};
use std::future::Future;

use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::sleep;

// =============================================================================
// Skip-if-disabled macros for test utilities
// =============================================================================

/// Check if a tool is disabled in the config or environment.
/// Returns true if:
/// 1. Environment variable `AHMA_DISABLE_TOOL_{TOOL_NAME_UPPER}` is "true" or "1"
/// 2. The tool JSON exists in `.ahma/` and has `"enabled": false`
/// 3. The tool JSON exists in `ahma_mcp/examples/configs/` and has `"enabled": false`
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
            .join("ahma_mcp/examples/configs")
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
/// use ahma_mcp::skip_if_disabled;
///
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

// Common test utilities from tests/common/mod.rs

use crate::adapter::Adapter;
use crate::operation_monitor::{MonitorConfig, OperationMonitor};
use crate::shell_pool::{ShellPoolConfig, ShellPoolManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Get the workspace directory for tests
#[allow(dead_code)]
pub fn get_workspace_dir() -> PathBuf {
    // In a workspace, CARGO_MANIFEST_DIR points to the crate directory (ahma_mcp)
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

/// Wait for an async condition to become true, polling at a fixed interval.
/// Returns true if the condition succeeds within the timeout.
pub async fn wait_for_condition<F, Fut>(
    timeout: Duration,
    interval: Duration,
    mut condition: F,
) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = tokio::time::Instant::now();
    loop {
        if condition().await {
            return true;
        }

        if start.elapsed() >= timeout {
            return false;
        }

        tokio::time::sleep(interval).await;
    }
}

/// Wait for an operation to reach a terminal state, checking both active and completed history.
pub async fn wait_for_operation_terminal(
    monitor: &crate::operation_monitor::OperationMonitor,
    operation_id: &str,
    timeout: Duration,
    interval: Duration,
) -> bool {
    wait_for_condition(timeout, interval, || {
        let monitor = monitor.clone();
        let operation_id = operation_id.to_string();
        async move {
            if let Some(op) = monitor.get_operation(&operation_id).await {
                return op.state.is_terminal();
            }

            let completed = monitor.get_completed_operations().await;
            completed.iter().any(|op| op.id == operation_id)
        }
    })
    .await
}

/// Helpers for CI-resilient stdio MCP integration tests.
///
/// # CI-Resilient Stdio Testing Patterns
///
/// ## Key Insight
/// In stdio MCP transport, notifications and responses share the same channel.
/// When a response arrives, the transport may start tearing down almost immediately.
/// Notifications in-flight may never be delivered to the client callback.
///
/// ## Rule 1: Prefer synchronous operations for notification tests
/// If testing that notifications are sent, use `synchronous: true` tools.
/// Notifications are sent during execution, before the response.
///
/// ## Rule 2: Don't race response completion against notification
/// Never use `tokio::select!` to race response vs notification - when the
/// response branch wins, the transport may already be closing.
///
/// ## Rule 3: Use generous timeouts on CI
/// CI environments (especially with concurrent nextest) are slower and more
/// variable. Use 10s+ timeouts for notification waiting.
pub mod stdio_test_helpers {
    use std::time::Duration;
    use tokio::sync::mpsc;

    /// Default timeout for CI-resilient notification waiting (10 seconds).
    ///
    /// CI environments are slower due to:
    /// - Concurrent test execution (nextest runs tests in parallel)
    /// - Shared resources and CPU contention
    /// - Variable performance characteristics
    pub const CI_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(10);

    /// Wait for a notification on an mpsc channel with timeout.
    ///
    /// Returns `Some(notification)` if received within timeout, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let notification = wait_for_notification(&mut rx, CI_NOTIFICATION_TIMEOUT).await;
    /// assert!(notification.is_some(), "Expected notification within timeout");
    /// ```
    pub async fn wait_for_notification<T>(
        rx: &mut mpsc::Receiver<T>,
        timeout: Duration,
    ) -> Option<T>
    where
        T: Send,
    {
        tokio::time::timeout(timeout, rx.recv())
            .await
            .ok()
            .flatten()
    }

    /// Wait for a notification matching a predicate with timeout.
    ///
    /// Keeps receiving notifications until one matches or timeout expires.
    /// Non-matching notifications are discarded.
    pub async fn wait_for_notification_matching<T, F>(
        rx: &mut mpsc::Receiver<T>,
        timeout: Duration,
        predicate: F,
    ) -> Option<T>
    where
        T: Send,
        F: Fn(&T) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(item)) if predicate(&item) => return Some(item),
                Ok(Some(_)) => continue, // Non-matching, try again
                Ok(None) => return None, // Channel closed
                Err(_) => return None,   // Timeout
            }
        }
    }
}

/// CLI testing helpers for reusing cached binaries and shared flags.
pub mod cli {
    use super::get_workspace_dir;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};

    /// Cached binary paths to avoid redundant builds across tests.
    /// Key: (package, binary) tuple as string "package:binary"
    static BINARY_CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

    /// Get the path to a binary in the target directory, resolving CARGO_TARGET_DIR correctly.
    ///
    /// This function handles relative `CARGO_TARGET_DIR` paths (e.g., `target`) by resolving
    /// them relative to the workspace root. This is critical for CI environments that set
    /// `CARGO_TARGET_DIR` to a relative path.
    ///
    /// Does NOT build the binary - caller is responsible for ensuring it exists.
    /// For automatic building with caching, use `build_binary_cached()` instead.
    pub fn get_binary_path(_package: &str, binary: &str) -> PathBuf {
        let workspace = get_workspace_dir();
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .map(|p| {
                if p.is_absolute() {
                    p
                } else {
                    workspace.join(p)
                }
            })
            .unwrap_or_else(|_| workspace.join("target"));

        target_dir.join("debug").join(binary)
    }

    /// Get or build a binary, caching the result.
    ///
    /// This function is optimized for test performance:
    /// 1. First checks if the binary already exists (common when running via cargo test/nextest)
    /// 2. Only builds if the binary doesn't exist
    /// 3. Caches the path to avoid redundant filesystem checks
    pub fn build_binary_cached(package: &str, binary: &str) -> PathBuf {
        let cache = BINARY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = format!("{}:{}", package, binary);

        // Fast path: check cache first
        {
            let cache_guard = cache.lock().unwrap();
            if let Some(path) = cache_guard.get(&key) {
                return path.clone();
            }
        }

        let binary_path = get_binary_path(package, binary);

        if binary_path.exists() {
            let mut cache_guard = cache.lock().unwrap();
            cache_guard.insert(key, binary_path.clone());
            return binary_path;
        }

        let workspace = get_workspace_dir();
        let output = Command::new("cargo")
            .current_dir(&workspace)
            .args(["build", "--package", package, "--bin", binary])
            .output()
            .expect("Failed to run cargo build");

        assert!(
            output.status.success(),
            "Failed to build {}: {}",
            binary,
            String::from_utf8_lossy(&output.stderr)
        );

        let mut cache_guard = cache.lock().unwrap();
        cache_guard.insert(key, binary_path.clone());

        binary_path
    }

    /// Create a command for a binary with test mode enabled (bypasses sandbox checks)
    pub fn test_command(binary: &PathBuf) -> Command {
        let mut cmd = Command::new(binary);
        cmd.env("AHMA_TEST_MODE", "1");
        cmd
    }
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

/// Test client helpers for spawning and interacting with the MCP server.
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
                        .current_dir(working_dir)
                        .kill_on_drop(true);
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
                        .arg("ahma_mcp")
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
                        // Keep tests deterministic even if the developer/CI environment sets these.
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

    /// Get the absolute path to the workspace tools directory
    #[allow(dead_code)]
    pub fn get_workspace_tools_dir() -> std::path::PathBuf {
        get_workspace_path(".ahma")
    }
}

// Test project module
/// Temporary project scaffolding helpers for integration tests.
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

/// Shared test helper functions used across crates.
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

    /// Initialize verbose logging for tests.
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

// =============================================================================
// Concurrent Testing Helpers
// =============================================================================
//
// These helpers prevent common CI failures caused by race conditions,
// improper timeouts, and duplicate notification handling during concurrent tests.
//
// WHY THESE EXIST:
// - AI-generated tests often introduce subtle race conditions
// - Fixed sleeps cause flaky tests on variable CI infrastructure
// - Concurrent notification tests can miss or duplicate notifications
// - Barrier-synchronized spawning ensures deterministic test starts

/// Helpers for writing CI-resilient concurrent tests.
///
/// # Common Mistakes This Module Prevents
///
/// 1. **Fixed sleep() calls**: Use `with_ci_timeout()` or `wait_for_condition()`
/// 2. **Racing notification handling**: Use `spawn_tasks_with_barrier()` for synchronized starts
/// 3. **Duplicate notifications**: Use `collect_unique_results()` with HashSet tracking
/// 4. **Unbounded concurrency**: Use `spawn_bounded_concurrent()` with limits
///
/// # Example: Safe Concurrent Operation Test
///
/// ```rust,ignore
/// use ahma_mcp::test_utils::concurrent_test_helpers::*;
///
/// #[tokio::test]
/// async fn test_concurrent_operations() -> anyhow::Result<()> {
///     let monitor = Arc::new(OperationMonitor::new(default_config()));
///     
///     // Spawn tasks that all start at the same time
///     let results = spawn_tasks_with_barrier(5, |task_id| {
///         let monitor = monitor.clone();
///         async move {
///             monitor.add_operation(Operation::new(
///                 format!("op-{}", task_id),
///                 "test".to_string(),
///                 "concurrent test".to_string(),
///                 None,
///             )).await;
///             task_id
///         }
///     }).await?;
///     
///     // Verify all tasks completed with unique results
///     assert_all_unique(&results);
///     Ok(())
/// }
/// ```
pub mod concurrent_test_helpers {
    use std::collections::HashSet;
    use std::future::Future;
    use std::hash::Hash;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Barrier;

    /// Default CI-friendly timeout for concurrent operations.
    /// CI environments are slower due to resource contention.
    pub const CI_DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Generous timeout for heavy operations (compilation, network, etc.)
    pub const CI_HEAVY_TIMEOUT: Duration = Duration::from_secs(120);

    /// Short timeout for quick sanity checks
    pub const CI_QUICK_TIMEOUT: Duration = Duration::from_secs(5);

    /// Spawn multiple tasks that all start simultaneously using a barrier.
    ///
    /// This ensures deterministic concurrent behavior by synchronizing
    /// all task starts at the same instant. Useful for testing race conditions.
    ///
    /// # Arguments
    /// * `count` - Number of concurrent tasks to spawn
    /// * `task_fn` - Factory function that takes task index (0..count) and returns a future
    ///
    /// # Returns
    /// Vector of results from all tasks, in arbitrary order (due to concurrency)
    pub async fn spawn_tasks_with_barrier<F, Fut, T>(count: usize, task_fn: F) -> Vec<T>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let barrier = Arc::new(Barrier::new(count));
        let task_fn = Arc::new(task_fn);

        let handles: Vec<_> = (0..count)
            .map(|i| {
                let barrier = barrier.clone();
                let task_fn = task_fn.clone();
                tokio::spawn(async move {
                    // All tasks wait here until everyone is ready
                    barrier.wait().await;
                    task_fn(i).await
                })
            })
            .collect();

        let mut results = Vec::with_capacity(count);
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        results
    }

    /// Collect results from concurrent operations with a deadline.
    ///
    /// Returns early if all expected results arrive before timeout.
    /// Useful for notification collection where exact timing is unpredictable.
    ///
    /// # Arguments
    /// * `rx` - Receiver channel for results
    /// * `expected_count` - Number of results expected (returns early when reached)
    /// * `timeout` - Maximum time to wait for results
    pub async fn collect_results_with_deadline<T>(
        rx: &mut tokio::sync::mpsc::Receiver<T>,
        expected_count: usize,
        timeout: Duration,
    ) -> Vec<T>
    where
        T: Send,
    {
        let mut results = Vec::with_capacity(expected_count);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if results.len() >= expected_count {
                break;
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(result)) => results.push(result),
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout
            }
        }

        results
    }

    /// Assert that all items in a collection are unique.
    ///
    /// Use this to verify that notifications or operations were not duplicated.
    pub fn assert_all_unique<T: Eq + Hash + std::fmt::Debug>(items: &[T]) {
        let mut seen = HashSet::new();
        for item in items {
            assert!(
                seen.insert(item),
                "Duplicate item detected: {:?}. This indicates a race condition or notification bug.",
                item
            );
        }
    }

    /// Assert that a collection has no duplicates when mapped by a key function.
    ///
    /// Useful when comparing complex objects by a specific field (e.g., operation ID).
    pub fn assert_unique_by<T, K, F>(items: &[T], key_fn: F)
    where
        K: Eq + Hash + std::fmt::Debug,
        F: Fn(&T) -> K,
    {
        let mut seen = HashSet::new();
        for item in items {
            let key = key_fn(item);
            assert!(
                seen.insert(key),
                "Duplicate key detected. This indicates a race condition or notification bug."
            );
        }
    }

    /// Wrap a future with a CI-appropriate timeout.
    ///
    /// Provides clear error messages when timeout occurs, helping diagnose
    /// CI-specific hangs.
    ///
    /// # Example
    /// ```rust,ignore
    /// let result = with_ci_timeout(
    ///     "operation completion",
    ///     CI_DEFAULT_TIMEOUT,
    ///     async { monitor.wait_for_operation("op-1").await }
    /// ).await?;
    /// ```
    pub async fn with_ci_timeout<T, Fut>(
        operation_name: &str,
        timeout: Duration,
        fut: Fut,
    ) -> anyhow::Result<T>
    where
        Fut: Future<Output = T>,
    {
        tokio::time::timeout(timeout, fut).await.map_err(|_| {
            anyhow::anyhow!(
                "CI timeout: '{}' did not complete within {:?}. \
                 This may indicate a deadlock, resource starvation, or slow CI environment.",
                operation_name,
                timeout
            )
        })
    }

    /// Wait for a condition with exponential backoff polling.
    ///
    /// More efficient than fixed-interval polling for conditions that
    /// may resolve quickly or take a long time.
    ///
    /// # Arguments
    /// * `description` - Human-readable description for error messages
    /// * `timeout` - Maximum time to wait
    /// * `condition` - Async function returning true when condition is met
    pub async fn wait_with_backoff<F, Fut>(
        description: &str,
        timeout: Duration,
        mut condition: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        let start = tokio::time::Instant::now();
        let mut interval = Duration::from_millis(10);
        let max_interval = Duration::from_millis(500);

        loop {
            if condition().await {
                return Ok(());
            }

            if start.elapsed() >= timeout {
                return Err(anyhow::anyhow!(
                    "Condition '{}' not met within {:?}. This may indicate a bug or slow CI.",
                    description,
                    timeout
                ));
            }

            tokio::time::sleep(interval).await;
            interval = (interval * 2).min(max_interval);
        }
    }

    /// Spawn concurrent tasks with a bounded limit to prevent resource exhaustion.
    ///
    /// CI environments often have limited resources. This helper ensures
    /// tests don't spawn unbounded tasks that could cause OOM or timeouts.
    ///
    /// # Arguments
    /// * `items` - Iterator of items to process
    /// * `concurrency_limit` - Maximum concurrent tasks
    /// * `task_fn` - Function to apply to each item
    pub async fn spawn_bounded_concurrent<I, T, F, Fut, R>(
        items: I,
        concurrency_limit: usize,
        task_fn: F,
    ) -> Vec<R>
    where
        I: IntoIterator<Item = T>,
        T: Send + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        use tokio::sync::Semaphore;

        let semaphore = Arc::new(Semaphore::new(concurrency_limit));
        let task_fn = Arc::new(task_fn);

        let handles: Vec<_> = items
            .into_iter()
            .map(|item| {
                let semaphore = semaphore.clone();
                let task_fn = task_fn.clone();
                tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    task_fn(item).await
                })
            })
            .collect();

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        results
    }
}

// =============================================================================
// Async Test Assertion Helpers
// =============================================================================

/// Helpers for making assertions in async tests more CI-resilient.
pub mod async_assertions {
    use std::future::Future;
    use std::time::Duration;

    /// Assert that an async operation completes within the expected time.
    ///
    /// # Example
    /// ```rust,ignore
    /// assert_completes_within(
    ///     Duration::from_secs(5),
    ///     "operation should complete quickly",
    ///     async { monitor.get_status("op-1").await }
    /// ).await;
    /// ```
    pub async fn assert_completes_within<T, Fut>(
        timeout: Duration,
        description: &str,
        fut: Fut,
    ) -> T
    where
        Fut: Future<Output = T>,
    {
        tokio::time::timeout(timeout, fut)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "Assertion failed: {} did not complete within {:?}",
                    description, timeout
                )
            })
    }

    /// Assert that an async operation times out (does not complete).
    ///
    /// Useful for testing that blocking operations properly block.
    pub async fn assert_times_out<T, Fut>(timeout: Duration, description: &str, fut: Fut)
    where
        Fut: Future<Output = T>,
    {
        let result = tokio::time::timeout(timeout, fut).await;
        assert!(
            result.is_err(),
            "Expected {} to timeout, but it completed",
            description
        );
    }

    /// Assert that a condition becomes true within a timeout.
    ///
    /// Polls the condition and provides clear failure messages.
    pub async fn assert_eventually<F, Fut>(
        timeout: Duration,
        poll_interval: Duration,
        description: &str,
        mut condition: F,
    ) where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if condition().await {
                return;
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!(
                    "Assertion failed: '{}' did not become true within {:?}",
                    description, timeout
                );
            }

            tokio::time::sleep(poll_interval.min(remaining)).await;
        }
    }
}

// =============================================================================
// HTTP Bridge Test Helpers
// =============================================================================

/// A running HTTP bridge instance for integration testing.
pub struct HttpBridgeTestInstance {
    pub child: std::process::Child,
    pub port: u16,
    pub temp_dir: TempDir,
}

impl HttpBridgeTestInstance {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for HttpBridgeTestInstance {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Spawn a robust HTTP bridge for testing.
pub async fn spawn_http_bridge() -> anyhow::Result<HttpBridgeTestInstance> {
    use std::net::TcpListener;
    use std::process::{Command, Stdio};

    // Find available port
    let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();

    let binary = cli::build_binary_cached("ahma_mcp", "ahma_mcp");

    let temp_dir = TempDir::new()?;
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir)?;

    let mut child = Command::new(&binary)
        .args([
            "--mode",
            "http",
            "--http-port",
            &port.to_string(),
            "--tools-dir",
            &tools_dir.to_string_lossy(),
            "--sandbox-scope",
            &temp_dir.path().to_string_lossy(),
            "--log-to-stderr",
        ])
        .env_remove("AHMA_TEST_MODE")
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_EXECUTION_MODE")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUST_TEST_THREADS")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for server health
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", port);

    let start = Instant::now();
    let timeout = Duration::from_secs(10);

    while start.elapsed() < timeout {
        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                return Ok(HttpBridgeTestInstance {
                    child,
                    port,
                    temp_dir,
                });
            }
        }
        sleep(Duration::from_millis(100)).await;
    }

    let _ = child.kill();
    let _ = child.wait();
    anyhow::bail!("Timed out waiting for HTTP bridge health");
}

/// A client for testing the MCP protocol over HTTP and SSE.
pub struct HttpMcpTestClient {
    pub client: reqwest::Client,
    pub base_url: String,
    pub session_id: Option<String>,
}

impl HttpMcpTestClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            session_id: None,
        }
    }

    pub async fn send_request(
        &self,
        request: &serde_json::Value,
    ) -> anyhow::Result<(serde_json::Value, Option<String>)> {
        let url = format!("{}/mcp", self.base_url);
        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid);
        }

        let resp = req.json(request).send().await.context("POST /mcp failed")?;

        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .or_else(|| resp.headers().get("Mcp-Session-Id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body: serde_json::Value = resp.json().await.context("Failed to parse JSON response")?;

        Ok((body, session_id))
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let init_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test-client", "version": "1.0.0"}
            }
        });

        let (resp, sid) = self.send_request(&init_request).await?;
        if let Some(err) = resp.get("error") {
            anyhow::bail!("Initialize failed: {:?}", err);
        }
        self.session_id = sid;

        let initialized = serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        self.send_request(&initialized).await?;

        Ok(())
    }

    pub async fn start_sse_events(
        &self,
        roots: Vec<std::path::PathBuf>,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<serde_json::Value>> {
        use futures::StreamExt;

        let sid = self.session_id.as_ref().context("Not initialized")?.clone();
        let url = format!("{}/mcp", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Mcp-Session-Id", &sid)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("SSE failed: {}", resp.status());
        }

        let mut stream = resp.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::channel::<serde_json::Value>(256);
        let client = self.client.clone();
        let base_url = self.base_url.clone();

        tokio::spawn(async move {
            let mut buffer = String::new();
            loop {
                let chunk = match stream.next().await {
                    Some(Ok(c)) => c,
                    _ => break,
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(idx) = buffer.find("\n\n") {
                    let raw_event = buffer[..idx].to_string();
                    buffer = buffer[idx + 2..].to_string();

                    let mut data_lines = Vec::new();
                    for line in raw_event.lines() {
                        let line = line.trim_end_matches('\r');
                        if let Some(rest) = line.strip_prefix("data:") {
                            data_lines.push(rest.trim());
                        }
                    }
                    if data_lines.is_empty() {
                        continue;
                    }

                    let data = data_lines.join("\n");
                    let value: serde_json::Value = match serde_json::from_str(&data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    if value.get("method").and_then(|m| m.as_str()) == Some("roots/list") {
                        let id = value.get("id").cloned().expect("roots/list must have id");
                        let roots_json: Vec<serde_json::Value> = roots
                            .iter()
                            .map(|p| {
                                serde_json::json!({
                                    "uri": format!("file://{}", p.display()),
                                    "name": p.file_name().and_then(|n| n.to_str()).unwrap_or("root")
                                })
                            })
                            .collect();
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "roots": roots_json }
                        });

                        let _ = client
                            .post(format!("{}/mcp", base_url))
                            .header("Mcp-Session-Id", &sid)
                            .json(&response)
                            .send()
                            .await;
                    }
                    let _ = tx.send(value).await;
                }
            }
        });

        Ok(rx)
    }
}
