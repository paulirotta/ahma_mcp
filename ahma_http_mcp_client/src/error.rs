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
    #[error("OAuth2 configuration error: {0}")]
    OAuth2Config(#[from] oauth2::ConfigurationError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Missing access token")]
    MissingAccessToken,
    #[error("RPC endpoint not announced yet")]
    MissingRpcEndpoint,
    #[error("Token refresh failed")]
    TokenRefreshFailed,
    #[error("Custom error: {0}")]
    Custom(String),
}

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
