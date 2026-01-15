use thiserror::Error;

/// Errors that can occur when using the HTTP MCP client.
#[derive(Error, Debug)]
pub enum McpHttpError {
    /// Error occurring during an HTTP request via `reqwest`.
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    /// Error serializing or deserializing JSON data.
    #[error("JSON serialization/deserialization failed: {0}")]
    Json(#[from] serde_json::Error),

    /// Error parsing a URL.
    #[error("URL parsing failed: {0}")]
    UrlParse(#[from] url::ParseError),

    /// Generic OAuth2 error message.
    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    /// Error in OAuth2 configuration.
    #[error("OAuth2 configuration error: {0}")]
    OAuth2Config(#[from] oauth2::ConfigurationError),

    /// lower-level I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Authentication failure message.
    #[error("Authentication failed: {0}")]
    Auth(String),

    /// An expected access token was missing.
    #[error("Missing access token")]
    MissingAccessToken,

    /// The RPC endpoint was not found or announced.
    #[error("RPC endpoint not announced yet")]
    MissingRpcEndpoint,

    /// Failed to refresh the OAuth2 token.
    #[error("Token refresh failed")]
    TokenRefreshFailed,

    /// Custom error message.
    #[error("Custom error: {0}")]
    Custom(String),
}

/// A specialized Result type for HTTP MCP Client operations.
pub type Result<T> = std::result::Result<T, McpHttpError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth2_error_display() {
        let err = McpHttpError::OAuth2("token expired".to_string());
        assert_eq!(err.to_string(), "OAuth2 error: token expired");
    }

    #[test]
    fn auth_error_display() {
        let err = McpHttpError::Auth("invalid credentials".to_string());
        assert_eq!(
            err.to_string(),
            "Authentication failed: invalid credentials"
        );
    }

    #[test]
    fn missing_access_token_display() {
        let err = McpHttpError::MissingAccessToken;
        assert_eq!(err.to_string(), "Missing access token");
    }

    #[test]
    fn missing_rpc_endpoint_display() {
        let err = McpHttpError::MissingRpcEndpoint;
        assert_eq!(err.to_string(), "RPC endpoint not announced yet");
    }

    #[test]
    fn token_refresh_failed_display() {
        let err = McpHttpError::TokenRefreshFailed;
        assert_eq!(err.to_string(), "Token refresh failed");
    }

    #[test]
    fn custom_error_display() {
        let err = McpHttpError::Custom("something went wrong".to_string());
        assert_eq!(err.to_string(), "Custom error: something went wrong");
    }

    #[test]
    fn url_parse_error_conversion() {
        let url_err = url::Url::parse("not a valid url").unwrap_err();
        let err: McpHttpError = url_err.into();
        assert!(err.to_string().contains("URL parsing failed"));
    }

    #[test]
    fn json_error_conversion() {
        let json_err: serde_json::Error = serde_json::from_str::<i32>("not json").unwrap_err();
        let err: McpHttpError = json_err.into();
        assert!(err.to_string().contains("JSON"));
    }

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: McpHttpError = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn error_is_debug() {
        let err = McpHttpError::MissingAccessToken;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("MissingAccessToken"));
    }
}
