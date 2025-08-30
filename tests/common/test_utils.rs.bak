/// Test utilities for ahma_mcp testing
use std::path::Path;

/// Check if output contains any of the expected patterns
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
            && let Some(name) = line.split(':').nth(1) {
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

/// Read a file and return its contents as a string
pub async fn read_file_contents(path: &Path) -> anyhow::Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Write contents to a file
pub async fn write_file_contents(path: &Path, contents: &str) -> anyhow::Result<()> {
    Ok(tokio::fs::write(path, contents).await?)
}
