use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpHttpError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),
    #[error("JSON serialization/deserialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("URL parsing failed: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("OAuth2 error: {0}")]
    OAuth2(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP error: {0}")]
    Mcp(#[from] rmcp::McpError),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Missing access token")]
    MissingAccessToken,
    #[error("Token refresh failed")]
    TokenRefreshFailed,
    #[error("Custom error: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, McpHttpError>;
