//! Ahma HTTP MCP Server - Main binary

use ahma_core::AhmaMcpService;
use ahma_http_mcp_server::{Protocol, ServerConfig, start_server};
use anyhow::{Context, Result};
use clap::Parser;
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "ahma_http_mcp_server")]
#[command(author, version, about = "HTTP/3 and HTTP/2 MCP Server", long_about = None)]
struct Args {
    /// Protocol to use (http2, http3, http3-fallback, both)
    #[arg(short, long, default_value = "http3-fallback")]
    protocol: String,
    
    /// HTTP/2 bind address
    #[arg(long, default_value = "127.0.0.1:3000")]
    http2_addr: SocketAddr,
    
    /// HTTP/3 bind address
    #[arg(long, default_value = "127.0.0.1:4433")]
    http3_addr: SocketAddr,
    
    /// Disable TLS (not recommended, for testing only)
    #[arg(long)]
    no_tls: bool,
    
    /// Path to tool configuration directory
    #[arg(short, long)]
    config_dir: Option<String>,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize tracing
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::from_default_env()
                .add_directive(log_level.into())
        )
        .init();
    
    info!("Starting Ahma HTTP MCP Server");
    info!("Protocol: {}", args.protocol);
    info!("HTTP/2 address: {}", args.http2_addr);
    info!("HTTP/3 address: {}", args.http3_addr);
    info!("TLS enabled: {}", !args.no_tls);
    
    // Parse protocol
    let protocol = match args.protocol.to_lowercase().as_str() {
        "http2" => Protocol::Http2,
        "http3" => Protocol::Http3,
        "http3-fallback" => Protocol::Http3WithFallback,
        "both" => Protocol::Both,
        _ => {
            anyhow::bail!("Invalid protocol: {}. Use http2, http3, http3-fallback, or both", args.protocol);
        }
    };
    
    // Load tool configurations
    let config_paths = if let Some(config_dir) = args.config_dir {
        vec![config_dir]
    } else {
        // Use default config locations
        vec![]
    };
    
    // Create MCP service
    let mcp_service = AhmaMcpService::new(config_paths)
        .context("Failed to create MCP service")?;
    
    info!("MCP service initialized");
    
    // Create server config
    let server_config = ServerConfig {
        protocol,
        http2_addr: args.http2_addr,
        http3_addr: args.http3_addr,
        enable_tls: !args.no_tls,
    };
    
    // Start the server
    info!("Server starting...");
    start_server(server_config, mcp_service).await?;
    
    Ok(())
}

