//! Error types for the HTTP bridge

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Server process error: {0}")]
    ServerProcess(String),
    
    #[error("Communication error: {0}")]
    Communication(String),
    
    #[error("HTTP server error: {0}")]
    HttpServer(String),
}

pub type Result<T> = std::result::Result<T, BridgeError>;

