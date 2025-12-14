//! Error types for the HTTP MCP server

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Certificate error: {0}")]
    Certificate(#[from] anyhow::Error),
    
    #[error("HTTP/3 error: {0}")]
    Http3(String),
    
    #[error("HTTP/2 error: {0}")]
    Http2(String),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("MCP error: {0}")]
    Mcp(String),
    
    #[error("Transport error: {0}")]
    Transport(String),
    
    #[error("Server error: {0}")]
    Server(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ServerError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ServerError::Certificate(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ServerError::Http3(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
            ServerError::Http2(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
            ServerError::Json(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ServerError::Mcp(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
            ServerError::Transport(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
            ServerError::Server(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
        };

        let body = Json(json!({
            "error": {
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, ServerError>;

