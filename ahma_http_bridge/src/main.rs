use ahma_http_bridge::{BridgeConfig, start_bridge};
use std::path::Path;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut config = BridgeConfig::default();

    // Auto-detect local binary for development
    let cwd = std::env::current_dir()?;
    if let Some(local_binary) = detect_local_debug_binary(&cwd) {
        config.server_command = local_binary;
    }

    tracing::info!("Starting Ahma HTTP Bridge on {}", config.bind_addr);
    tracing::info!("Proxying to command: {}", config.server_command);

    start_bridge(config).await?;
    Ok(())
}

fn detect_local_debug_binary(base_dir: &Path) -> Option<String> {
    let candidate = base_dir.join("target").join("debug").join("ahma_mcp");
    if candidate.exists() {
        Some(candidate.to_string_lossy().into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detect_local_debug_binary_finds_existing_path() {
        let tmp = tempdir().unwrap();
        let binary_path = tmp.path().join("target").join("debug").join("ahma_mcp");
        fs::create_dir_all(binary_path.parent().unwrap()).unwrap();
        fs::write(&binary_path, b"test").unwrap();

        let detected = detect_local_debug_binary(tmp.path());
        assert_eq!(detected.as_deref(), binary_path.to_str());
    }

    #[test]
    fn detect_local_debug_binary_returns_none_when_missing() {
        let tmp = tempdir().unwrap();
        assert!(detect_local_debug_binary(tmp.path()).is_none());
    }
}
