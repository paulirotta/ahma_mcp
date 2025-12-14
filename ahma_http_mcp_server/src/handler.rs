//! MCP protocol handlers for HTTP requests

use crate::error::{Result, ServerError};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rmcp::{
    RoleServer, Service,
    handler::server::ServerHandler,
    model::{JsonRpcResponse, JsonRpcVersion2_0},
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
};
// Try to find Server
use rmcp::McpServer;

use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error};

/// Shared state for the MCP server
#[derive(Clone)]
pub struct McpServerState<H: ServerHandler> {
    pub server: Arc<Mutex<Server<H>>>,
}

impl<H: ServerHandler> McpServerState<H> {
    pub fn new(handler: H) -> Self {
        Self {
            server: Arc::new(Mutex::new(Server::new(handler))),
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
    let message: RxJsonRpcMessage<RoleServer> =
        serde_json::from_value(payload).map_err(|e| ServerError::Json(e))?;

    // Process the message through the server
    let mut server = state.server.lock().await;

    let response = match message {
        RxJsonRpcMessage::Request(req) => {
            debug!("Processing request");

            // Handle the request using the Service trait
            match server.call(req.request).await {
                Ok(response) => {
                    debug!("Request handled successfully");
                    TxJsonRpcMessage::Response(JsonRpcResponse {
                        jsonrpc: JsonRpcVersion2_0,
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

            if let Err(e) = server.call(notif.notification).await {
                error!("Error handling notification: {:?}", e);
            }

            // Notifications don't get responses
            return Ok((StatusCode::NO_CONTENT, ()).into_response());
        }
        RxJsonRpcMessage::Response(_) | RxJsonRpcMessage::Error(_) => {
            // These are responses from client, which shouldn't happen in server context
            return Err(ServerError::Mcp(
                "Unexpected response message from client".to_string(),
            ));
        }
    };

    // Send the response
    let json = serde_json::to_value(&response).map_err(|e| ServerError::Json(e))?;

    Ok(Json(json).into_response())
}

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
