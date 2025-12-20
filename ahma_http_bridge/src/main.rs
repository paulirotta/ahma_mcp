use ahma_http_bridge::{BridgeConfig, start_bridge};
use clap::Parser;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// HTTP-to-stdio bridge for MCP servers with session isolation support.
///
/// Enables multiple IDE instances to share a single HTTP endpoint while maintaining
/// separate sandbox scopes based on each client's workspace roots.
#[derive(Parser, Debug)]
#[command(name = "ahma_http_bridge")]
#[command(version, about)]
struct Args {
    /// Enable session isolation mode (separate subprocess per client).
    /// Each client gets its own sandbox scope derived from its workspace roots.
    /// Recommended for multi-project usage.
    #[arg(long, default_value_t = true)]
    session_isolation: bool,

    /// Disable session isolation (single shared subprocess for all clients).
    /// Use this if all clients share the same project directory.
    #[arg(long, conflicts_with = "session_isolation")]
    no_session_isolation: bool,

    /// Address to bind the HTTP server.
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind_addr: SocketAddr,

    /// Default sandbox scope if client provides no workspace roots.
    /// Only used in session isolation mode.
    #[arg(long, default_value = ".")]
    default_sandbox_scope: PathBuf,

    /// Command to run the MCP server subprocess.
    /// If not specified, auto-detects local debug binary or uses 'ahma_mcp'.
    #[arg(long)]
    server_command: Option<String>,

    /// Additional arguments to pass to the MCP server subprocess.
    #[arg(long)]
    server_args: Vec<String>,

    /// Enable colored terminal output for subprocess I/O (debug mode).
    #[arg(long)]
    colored_output: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Determine server command: explicit arg > local debug binary > default
    let cwd = std::env::current_dir()?;
    let (server_command, enable_colored_output) = match args.server_command {
        Some(cmd) => (cmd, args.colored_output),
        None => {
            if let Some(local_binary) = detect_local_debug_binary(&cwd) {
                tracing::info!("Debug mode detected - using local binary, colored output enabled");
                (local_binary, true)
            } else {
                ("ahma_mcp".to_string(), args.colored_output)
            }
        }
    };

    // Determine session isolation mode
    let session_isolation = !args.no_session_isolation;

    let config = BridgeConfig {
        bind_addr: args.bind_addr,
        server_command,
        server_args: args.server_args,
        enable_colored_output,
        session_isolation,
        default_sandbox_scope: args.default_sandbox_scope,
    };

    tracing::info!("Starting Ahma HTTP Bridge on {}", config.bind_addr);
    tracing::info!("Proxying to command: {}", config.server_command);
    tracing::info!(
        "Session isolation: {} (each client gets separate sandbox)",
        if config.session_isolation {
            "enabled"
        } else {
            "disabled"
        }
    );

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
