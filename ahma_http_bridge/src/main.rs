use ahma_http_bridge::{BridgeConfig, start_bridge};
use clap::Parser;
use opentelemetry::{KeyValue, trace::TracerProvider};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, trace as sdktrace};
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

    /// Block writes to temp directories (/tmp, /var/folders) for higher security.
    /// This prevents data exfiltration via temp files but breaks tools that require temp access.
    /// When enabled, passes --no-temp-files to all spawned MCP subprocesses.
    #[arg(long)]
    no_temp_files: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let env_filter = tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    );
    let otel_layer = init_otel();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(otel_layer)
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

    // Build server args, adding --no-temp-files if enabled
    let mut server_args = args.server_args;
    if args.no_temp_files {
        server_args.push("--no-temp-files".to_string());
        tracing::info!("🔒 High-security mode: temp file writes will be blocked in subprocesses");
    }

    let config = BridgeConfig {
        bind_addr: args.bind_addr,
        server_command,
        server_args,
        enable_colored_output,
        default_sandbox_scope: args.default_sandbox_scope,
    };

    tracing::info!("Starting Ahma HTTP Bridge on {}", config.bind_addr);
    tracing::info!("Proxying to command: {}", config.server_command);
    tracing::info!("Session isolation: ENABLED (always-on)");

    start_bridge(config).await?;
    Ok(())
}

fn init_otel<S>() -> Option<tracing_opentelemetry::OpenTelemetryLayer<S, sdktrace::Tracer>>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() || std::env::var("AHMA_TRACING").is_ok()
    {
        let exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint("http://localhost:4318/v1/traces");

        let provider = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(
                sdktrace::Config::default().with_resource(Resource::new(vec![KeyValue::new(
                    "service.name",
                    "ahma_http_bridge",
                )])),
            )
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .ok()?;

        let tracer = provider.tracer("ahma_http_bridge");

        Some(tracing_opentelemetry::layer().with_tracer(tracer))
    } else {
        None
    }
}

fn detect_local_debug_binary(base_dir: &Path) -> Option<String> {
    let binary_path = base_dir.join("target").join("debug").join("ahma_mcp");
    if binary_path.exists() {
        Some(binary_path.to_str()?.to_owned())
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
