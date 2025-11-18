//! Main server orchestration module

use crate::{
    error::{Result, ServerError},
    handler::McpServerState,
    http2_server::{Http2ServerConfig, start_http2_server},
    http3_server::{Http3ServerConfig, start_http3_server},
};
use rmcp::handler::server::ServerHandler;
use std::net::SocketAddr;
use tracing::{error, info};

/// Server protocol selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// HTTP/2 only
    Http2,
    /// HTTP/3 only
    Http3,
    /// Try HTTP/3, fallback to HTTP/2 if it fails
    Http3WithFallback,
    /// Run both HTTP/2 and HTTP/3 simultaneously
    Both,
}

/// Server configuration
pub struct ServerConfig {
    pub protocol: Protocol,
    pub http2_addr: SocketAddr,
    pub http3_addr: SocketAddr,
    pub enable_tls: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            protocol: Protocol::Http3WithFallback,
            http2_addr: "127.0.0.1:3000".parse().unwrap(),
            http3_addr: "127.0.0.1:4433".parse().unwrap(),
            enable_tls: true,
        }
    }
}

/// Start the MCP server with the given configuration
pub async fn start_server<H: ServerHandler + Send + Sync + Clone + 'static>(
    config: ServerConfig,
    handler: H,
) -> Result<()> {
    let state = McpServerState::new(handler);
    
    match config.protocol {
        Protocol::Http2 => {
            info!("Starting in HTTP/2 mode");
            let http2_config = Http2ServerConfig {
                bind_addr: config.http2_addr,
                enable_tls: config.enable_tls,
            };
            start_http2_server(http2_config, state).await
        }
        
        Protocol::Http3 => {
            info!("Starting in HTTP/3 mode");
            let http3_config = Http3ServerConfig {
                bind_addr: config.http3_addr,
            };
            start_http3_server(http3_config, state).await
        }
        
        Protocol::Http3WithFallback => {
            info!("Starting in HTTP/3 mode with HTTP/2 fallback");
            
            let http3_config = Http3ServerConfig {
                bind_addr: config.http3_addr,
            };
            
            // Try HTTP/3 first
            match start_http3_server(http3_config, state.clone()).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("HTTP/3 server failed: {}, falling back to HTTP/2", e);
                    
                    let http2_config = Http2ServerConfig {
                        bind_addr: config.http2_addr,
                        enable_tls: config.enable_tls,
                    };
                    start_http2_server(http2_config, state).await
                }
            }
        }
        
        Protocol::Both => {
            info!("Starting both HTTP/2 and HTTP/3 servers");
            
            let http2_state = state.clone();
            let http3_state = state;
            
            let http2_addr = config.http2_addr;
            let http3_addr = config.http3_addr;
            let enable_tls = config.enable_tls;
            
            // Start HTTP/2 server in background
            let http2_handle = tokio::spawn(async move {
                let http2_config = Http2ServerConfig {
                    bind_addr: http2_addr,
                    enable_tls,
                };
                
                if let Err(e) = start_http2_server(http2_config, http2_state).await {
                    error!("HTTP/2 server error: {}", e);
                }
            });
            
            // Start HTTP/3 server in background
            let http3_handle = tokio::spawn(async move {
                let http3_config = Http3ServerConfig {
                    bind_addr: http3_addr,
                };
                
                if let Err(e) = start_http3_server(http3_config, http3_state).await {
                    error!("HTTP/3 server error: {}", e);
                }
            });
            
            // Wait for both servers
            tokio::select! {
                result = http2_handle => {
                    match result {
                        Ok(_) => info!("HTTP/2 server stopped"),
                        Err(e) => error!("HTTP/2 server task error: {}", e),
                    }
                }
                result = http3_handle => {
                    match result {
                        Ok(_) => info!("HTTP/3 server stopped"),
                        Err(e) => error!("HTTP/3 server task error: {}", e),
                    }
                }
            }
            
            Err(ServerError::Server("One of the servers stopped".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.protocol, Protocol::Http3WithFallback);
        assert_eq!(config.http2_addr.to_string(), "127.0.0.1:3000");
        assert_eq!(config.http3_addr.to_string(), "127.0.0.1:4433");
        assert!(config.enable_tls);
    }
    
    #[test]
    fn test_protocol_variants() {
        assert_ne!(Protocol::Http2, Protocol::Http3);
        assert_ne!(Protocol::Http3, Protocol::Http3WithFallback);
        assert_ne!(Protocol::Both, Protocol::Http2);
    }
}

