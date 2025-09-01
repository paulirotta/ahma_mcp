pub mod test_project;
pub mod test_utils;

use ahma_mcp::adapter::Adapter;
use ahma_mcp::operation_monitor::{MonitorConfig, OperationMonitor};
use ahma_mcp::shell_pool::{ShellPoolConfig, ShellPoolManager};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Get the workspace directory for tests
#[allow(dead_code)]
pub fn get_workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Create a test config for integration tests
#[allow(dead_code)]
pub fn create_test_config(_workspace_dir: &PathBuf) -> Result<Arc<Adapter>> {
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
        command_timeout: Duration::from_secs(30), // 30 second timeout for tests
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
