//! Error types for the HTTP MCP server

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

pub type Result<T> = std::result::Result<T, ServerError>;

