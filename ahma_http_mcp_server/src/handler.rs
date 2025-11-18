//! MCP protocol handlers for HTTP requests

use crate::error::{Result, ServerError};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, Sse},
    Json,
};
use futures::stream::Stream;
use rmcp::{
    RoleServer, Service,
    handler::server::ServerHandler,
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
};
use serde_json::Value;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, info};

/// Shared state for the MCP server
#[derive(Clone)]
pub struct McpServerState<H: ServerHandler> {
    pub handler: Arc<Mutex<H>>,
    pub tx: Arc<Mutex<Option<mpsc::Sender<TxJsonRpcMessage<RoleServer>>>>>,
}

impl<H: ServerHandler> McpServerState<H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(Mutex::new(handler)),
            tx: Arc::new(Mutex::new(None)),
        }
    }
}

/// Handle POST requests for MCP messages
pub async fn handle_mcp_post<H: ServerHandler + Send + Sync + 'static>(
    State(state): State<McpServerState<H>>,
    Json(payload): Json<Value>,
) -> Result<Response> {
    debug!("Received MCP POST request");
    
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
            
            // Notifications don't get responses
            return Ok((StatusCode::NO_CONTENT, Body::empty()).into_response());
        }
        RxJsonRpcMessage::Response(_) | RxJsonRpcMessage::Error(_) => {
            // These are responses from client, which shouldn't happen in server context
            return Err(ServerError::Mcp("Unexpected response message from client".to_string()));
        }
    };
    
    // Send the response
    let json = serde_json::to_value(&response)
        .map_err(|e| ServerError::Json(e))?;
    
    Ok(Json(json).into_response())
}

/// Server-Sent Events stream for server-initiated messages
pub struct McpEventStream {
    rx: mpsc::Receiver<TxJsonRpcMessage<RoleServer>>,
}

impl Stream for McpEventStream {
    type Item = std::result::Result<axum::response::sse::Event, Infallible>;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(msg)) => {
                // Serialize the message to JSON
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        let event = axum::response::sse::Event::default().data(json);
                        Poll::Ready(Some(Ok(event)))
                    }
                    Err(e) => {
                        error!("Failed to serialize SSE message: {}", e);
                        Poll::Pending
                    }
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Handle SSE connection for server-initiated messages
pub async fn handle_mcp_sse<H: ServerHandler + Send + Sync + 'static>(
    State(state): State<McpServerState<H>>,
    _headers: HeaderMap,
) -> Sse<McpEventStream> {
    info!("Client connected to SSE endpoint");
    
    let (tx, rx) = mpsc::channel(100);
    
    // Store the sender for server-initiated messages
    *state.tx.lock().await = Some(tx);
    
    let stream = McpEventStream { rx };
    
    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(30))
                .text("keepalive")
        )
}

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::protocol::{Request, Response};
    
    // Mock handler for testing
    struct MockHandler;
    
    #[async_trait::async_trait]
    impl ServerHandler for MockHandler {
        type Error = String;
        
        async fn handle_request(&mut self, req: Request) -> std::result::Result<Response, Self::Error> {
            Ok(Response {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: serde_json::json!({"status": "ok"}),
            })
        }
        
        async fn handle_notification(&mut self, _notif: rmcp::protocol::Notification) -> std::result::Result<(), Self::Error> {
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_mcp_state_creation() {
        let handler = MockHandler;
        let state = McpServerState::new(handler);
        
        assert!(state.tx.lock().await.is_none());
    }
}

