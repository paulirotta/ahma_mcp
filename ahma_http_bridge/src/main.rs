use ahma_http_bridge::{BridgeConfig, start_bridge};
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
    if std::path::Path::new("target/debug/ahma_mcp").exists() {
        config.server_command = "target/debug/ahma_mcp".to_string();
    }

    tracing::info!("Starting Ahma HTTP Bridge on {}", config.bind_addr);
    tracing::info!("Proxying to command: {}", config.server_command);

    start_bridge(config).await?;
    Ok(())
}
