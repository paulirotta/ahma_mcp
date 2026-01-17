//! Error types for the HTTP bridge

use thiserror::Error;

/// Errors that can occur during bridge operation
#[derive(Error, Debug)]
pub enum BridgeError {
    /// Underlying IO failure
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization failure
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Failure managing the MCP server subprocess
    #[error("Server process error: {0}")]
    ServerProcess(String),

    /// Protocol or communication failure with subprocess
    #[error("Communication error: {0}")]
    Communication(String),

    /// HTTP server binding or runtime error
    #[error("HTTP server error: {0}")]
    HttpServer(String),
}

/// Convenience result type for bridge operations.
pub type Result<T> = std::result::Result<T, BridgeError>;
