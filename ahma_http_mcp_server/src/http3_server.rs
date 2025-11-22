//! HTTP/3 server implementation using Quinn and h3

use crate::{
    cert::{get_or_create_localhost_certs, load_certs_from_pem, load_private_key_from_pem},
    error::{Result, ServerError},
    handler::McpServerState,
};
use bytes::Bytes;
use h3::server::RequestStream;
use http::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use rmcp::handler::server::ServerHandler;
use std::{net::SocketAddr, sync::Arc};
use tracing::{debug, error, info, warn};

/// HTTP/3 server configuration
pub struct Http3ServerConfig {
    pub bind_addr: SocketAddr,
}

impl Default for Http3ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:4433".parse().unwrap(),
        }
    }
}

/// Start HTTP/3 server
pub async fn start_http3_server<H: ServerHandler + Send + Sync + Clone + 'static>(
    config: Http3ServerConfig,
    state: McpServerState<H>,
) -> Result<()> {
    info!("Starting HTTP/3 server on {}", config.bind_addr);
    
    // Get or create certificates
    let (cert_pem, key_pem) = get_or_create_localhost_certs(None)
        .await
        .map_err(|e| ServerError::Certificate(e))?;
    
    let certs = load_certs_from_pem(&cert_pem)
        .map_err(|e| ServerError::Certificate(e))?;
    
    let key = load_private_key_from_pem(&key_pem)
        .map_err(|e| ServerError::Certificate(e))?;
    
    // Configure rustls
    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| ServerError::Certificate(anyhow::anyhow!("TLS config error: {}", e)))?;
    
    tls_config.max_early_data_size = u32::MAX;
    tls_config.alpn_protocols = vec![b"h3".to_vec()];
    
    // Configure Quinn
    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
            .map_err(|e| ServerError::Certificate(anyhow::anyhow!("Quinn config error: {}", e)))?
    ));
    
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_bidi_streams(100_u32.into());
    transport_config.max_concurrent_uni_streams(100_u32.into());
    server_config.transport_config(Arc::new(transport_config));
    
    // Bind the endpoint
    let endpoint = quinn::Endpoint::server(server_config, config.bind_addr)
        .map_err(|e| ServerError::Http3(format!("Failed to bind endpoint: {}", e)))?;
    
    info!("HTTP/3 server listening on {}", config.bind_addr);
    info!("Note: Clients must accept self-signed certificates for localhost");
    
    // Accept connections
    while let Some(conn) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(conn, state).await {
                error!("Connection error: {}", e);
            }
        });
    }
    
    Ok(())
}

async fn handle_connection<H: ServerHandler + Send + Sync + Clone + 'static>(
    conn: quinn::Incoming,
    state: McpServerState<H>,
) -> Result<()> {
    let connection = conn.await
        .map_err(|e| ServerError::Http3(format!("Connection failed: {}", e)))?;
    
    debug!("New HTTP/3 connection from {}", connection.remote_address());
    
    let mut h3_conn = h3::server::Connection::new(h3_quinn::Connection::new(connection))
        .await
        .map_err(|e| ServerError::Http3(format!("H3 connection failed: {}", e)))?;
    
    loop {
        match h3_conn.accept().await {
            Ok(Some((req, stream))) => {
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_request(req, stream, state).await {
                        error!("Request handling error: {}", e);
                    }
                });
            }
            Ok(None) => {
                debug!("Connection closed");
                break;
            }
            Err(e) => {
                error!("Error accepting request: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}

async fn handle_request<H: ServerHandler + Send + Sync + Clone + 'static>(
    req: Request<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    state: McpServerState<H>,
) -> Result<()> {
    debug!("HTTP/3 request: {} {}", req.method(), req.uri());
    
    let path = req.uri().path();
    let method = req.method();
    
    // Route the request
    let response = match (method, path) {
        (&Method::GET, "/health") => {
            Response::builder()
                .status(StatusCode::OK)
                .body(())
                .unwrap()
        }
        (&Method::POST, "/mcp") => {
            // Read the request body
            let body = stream.recv_data().await
                .map_err(|e| ServerError::Http3(format!("Failed to read body: {}", e)))?;
            
            match body {
                Some(data) => {
                    // Parse JSON
                    let json: serde_json::Value = serde_json::from_slice(&data)
                        .map_err(|e| ServerError::Json(e))?;
                    
                    // Process through handler
                    match process_mcp_message(json, state, &mut stream).await {
                        Ok(_) => {
                            // Response already sent via stream
                            return Ok(());
                        }
                        Err(e) => {
                            error!("MCP processing error: {}", e);
                            Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(())
                                .unwrap()
                        }
                    }
                }
                None => {
                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(())
                        .unwrap()
                }
            }
        }
        (&Method::GET, "/mcp/sse") => {
            warn!("SSE not fully supported over HTTP/3, consider using HTTP/2");
            Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body(())
                .unwrap()
        }
        _ => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(())
                .unwrap()
        }
    };
    
    stream.send_response(response)
        .await
        .map_err(|e| ServerError::Http3(format!("Failed to send response: {}", e)))?;
    
    Ok(())
}

async fn process_mcp_message<H: ServerHandler + Send + Sync + Clone + 'static>(
    payload: serde_json::Value,
    state: McpServerState<H>,
    stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
) -> Result<()> {
    use rmcp::{RoleServer, Service, service::{RxJsonRpcMessage, TxJsonRpcMessage}};
    
    // Parse the incoming message
    let message: RxJsonRpcMessage<RoleServer> = serde_json::from_value(payload)
        .map_err(|e| ServerError::Json(e))?;
    
    // Process the message through the handler
    let mut handler = state.handler.lock().await;
    
    let response = match message {
        RxJsonRpcMessage::Request(req) => {
            debug!("Processing request");
            
            // Handle the request using the Service trait
            match handler.call(req.request).await {
                Ok(response) => {
                    debug!("Request handled successfully");
                    TxJsonRpcMessage::Response(rmcp::protocol::JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: serde_json::to_value(response).unwrap_or(serde_json::json!({})),
                    })
                }
                Err(e) => {
                    error!("Error handling request: {:?}", e);
                    return Err(ServerError::Mcp(format!("Handler error: {:?}", e)));
                }
            }
        }
        RxJsonRpcMessage::Notification(notif) => {
            debug!("Processing notification");
            
            if let Err(e) = handler.call(notif.notification).await {
                error!("Error handling notification: {:?}", e);
            }
            
            // Notifications don't get responses - just return OK
            return Ok(());
        }
        RxJsonRpcMessage::Response(_) | RxJsonRpcMessage::Error(_) => {
            return Err(ServerError::Mcp("Unexpected response message from client".to_string()));
        }
    };
    
    // Serialize and send response
    let response_json = serde_json::to_vec(&response)
        .map_err(|e| ServerError::Json(e))?;
    
    let http_response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(())
        .unwrap();
    
    stream.send_response(http_response)
        .await
        .map_err(|e| ServerError::Http3(format!("Failed to send response: {}", e)))?;
    
    stream.send_data(Bytes::from(response_json))
        .await
        .map_err(|e| ServerError::Http3(format!("Failed to send data: {}", e)))?;
    
    Ok(())
}

