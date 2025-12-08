//! HTTP/2 server implementation using Axum

use crate::{
    cert::{get_or_create_localhost_certs, load_certs_from_pem, load_private_key_from_pem},
    error::{Result, ServerError},
    handler::{handle_mcp_post, handle_mcp_sse, health_check, McpServerState},
};
use axum::{
    routing::{get, post},
    Router,
};
use rmcp::handler::server::ServerHandler;
use std::{net::SocketAddr, sync::Arc};
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
};
use tracing::info;

/// HTTP/2 server configuration
pub struct Http2ServerConfig {
    pub bind_addr: SocketAddr,
    pub enable_tls: bool,
}

impl Default for Http2ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            enable_tls: true,
        }
    }
}

/// Start HTTP/2 server with optional TLS
pub async fn start_http2_server<H: ServerHandler + Send + Sync + Clone + 'static>(
    config: Http2ServerConfig,
    state: McpServerState<H>,
) -> Result<()> {
    info!("Starting HTTP/2 server on {}", config.bind_addr);
    
    // Build the router
    // MCP Streamable HTTP transport: single endpoint supporting both POST (requests) and GET (SSE)
    // See: https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#streamable-http
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_post::<H>).get(handle_mcp_sse::<H>))
        .layer(
            CorsLayer::permissive()
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    
    if config.enable_tls {
        // Get or create certificates
        let (cert_pem, key_pem) = get_or_create_localhost_certs(None)
            .await
            .map_err(|e| ServerError::Certificate(e))?;
        
        let certs = load_certs_from_pem(&cert_pem)
            .map_err(|e| ServerError::Certificate(e))?;
        
        let key = load_private_key_from_pem(&key_pem)
            .map_err(|e| ServerError::Certificate(e))?;
        
        // Configure rustls for HTTP/2
        let mut tls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| ServerError::Certificate(anyhow::anyhow!("TLS config error: {}", e)))?;
        
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        
        let rustls_config = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(tls_config));
        
        info!("HTTP/2 server with TLS listening on https://{}", config.bind_addr);
        info!("Note: Clients must accept self-signed certificates for localhost");
        
        // Start the server with TLS
        axum_server::bind_rustls(config.bind_addr, rustls_config)
            .serve(app.into_make_service())
            .await
            .map_err(|e| ServerError::Http2(format!("Server error: {}", e)))?;
    } else {
        info!("HTTP/2 server (plaintext) listening on http://{}", config.bind_addr);
        
        // Start the server without TLS
        let listener = tokio::net::TcpListener::bind(config.bind_addr)
            .await
            .map_err(|e| ServerError::Io(e))?;
        
        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::Http2(format!("Server error: {}", e)))?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = Http2ServerConfig::default();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:3000");
        assert!(config.enable_tls);
    }
}

