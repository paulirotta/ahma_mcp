#![allow(dead_code)]
use ahma_core::{
    adapter::Adapter,
    client::Client,
    mcp_service::AhmaMcpService,
    operation_monitor::{MonitorConfig, OperationMonitor},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
};
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tempfile::{tempdir, TempDir};

pub fn init_test_logging() {
    let _ = ahma_core::utils::logging::init_logging("trace", false);
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
        if line.contains("Loading tool:") || line.contains("Tool loaded:") {
            if let Some(name) = line.split(':').nth(1) {
                tool_names.push(name.trim().to_string());
            }
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

/// Read a file and return its contents as a string
pub async fn read_file_contents(path: &Path) -> anyhow::Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Write contents to a file
pub async fn write_file_contents(path: &Path, contents: &str) -> anyhow::Result<()> {
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
pub async fn setup_test_environment_with_io(
) -> (AhmaMcpService, Sender<String>, Receiver<String>, TempDir) {
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
pub async fn setup_mcp_service_with_client() -> anyhow::Result<(TempDir, Client)> {
    let temp_dir = tempdir()?;
    let tools_dir = temp_dir.path().join("tools");
    tokio::fs::create_dir_all(&tools_dir).await?;

    let mut client = Client::new();
    client
        .start_process(Some(tools_dir.to_str().unwrap()))
        .await?;

    Ok((temp_dir, client))
}
