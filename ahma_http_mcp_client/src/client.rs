use crate::{
    error::{McpHttpError, Result},
    sse::{SseEvent, SseEventParser, resolve_rpc_url},
};
use futures::StreamExt;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl, basic::BasicClient,
};
use rmcp::{
    RoleClient,
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::Transport,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, mpsc};
use tracing::{error, info};
use url::Url;

type ConfiguredOAuthClient = oauth2::Client<
    oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
    oauth2::StandardTokenResponse<oauth2::EmptyExtraTokenFields, oauth2::basic::BasicTokenType>,
    oauth2::StandardTokenIntrospectionResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
    oauth2::StandardRevocableToken,
    oauth2::StandardErrorResponse<oauth2::RevocationErrorResponseType>,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

const RPC_ENDPOINT_WAIT_ATTEMPTS: usize = 300;
const RPC_ENDPOINT_WAIT_DELAY_MS: u64 = 100;
const TOKEN_FILE_NAME: &str = "mcp_http_token.json";
const TOKEN_PATH_ENV: &str = "AHMA_HTTP_CLIENT_TOKEN_PATH";
pub struct HttpMcpTransport {
    client: reqwest::Client,
    sse_url: Url,
    rpc_endpoint: Arc<Mutex<Option<Url>>>,
    token: Arc<Mutex<Option<StoredToken>>>,
    #[allow(dead_code)] // Will be used for token refresh
    oauth_client: Option<ConfiguredOAuthClient>,
    receiver: Arc<Mutex<mpsc::Receiver<RxJsonRpcMessage<RoleClient>>>>,
    sender: mpsc::Sender<RxJsonRpcMessage<RoleClient>>,
}

impl HttpMcpTransport {
    pub fn new(
        url: Url,
        atlassian_client_id: Option<String>,
        atlassian_client_secret: Option<String>,
    ) -> Result<Self> {
        let oauth_client = if let (Some(atlassian_client_id), Some(atlassian_client_secret)) =
            (atlassian_client_id, atlassian_client_secret)
        {
            let mut client = BasicClient::new(ClientId::new(atlassian_client_id))
                .set_client_secret(ClientSecret::new(atlassian_client_secret))
                .set_auth_uri(AuthUrl::new(
                    "https://auth.atlassian.com/authorize".to_string(),
                )?)
                .set_token_uri(TokenUrl::new(
                    "https://auth.atlassian.com/oauth/token".to_string(),
                )?);
            client =
                client.set_redirect_uri(RedirectUrl::new("http://localhost:8080".to_string())?);
            Some(client)
        } else {
            None
        };

        let (sender, receiver) = mpsc::channel(100);

        let transport = Self {
            client: reqwest::Client::new(),
            sse_url: url,
            rpc_endpoint: Arc::new(Mutex::new(None)),
            token: Arc::new(Mutex::new(load_token()?)),
            oauth_client,
            receiver: Arc::new(Mutex::new(receiver)),
            sender,
        };

        // Start SSE listener in background
        transport.start_sse_listener();

        Ok(transport)
    }

    /// Start a background task to listen for SSE messages from the server
    fn start_sse_listener(&self) {
        let sse_url = self.sse_url.clone();
        let token_arc = self.token.clone();
        let rpc_endpoint = self.rpc_endpoint.clone();
        let tx = self.sender.clone();
        let http_client = self.client.clone();

        tokio::spawn(async move {
            loop {
                let access_token = match token_arc.lock().await.as_ref() {
                    Some(t) => t.access_token.clone(),
                    None => {
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    }
                };

                let response = match http_client
                    .get(sse_url.clone())
                    .bearer_auth(&access_token)
                    .send()
                    .await
                {
                    Ok(res) if res.status().is_success() => res,
                    Ok(res) => {
                        error!("SSE connection failed with status: {}", res.status());
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        continue;
                    }
                    Err(e) => {
                        error!("SSE connection failed: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                use futures::TryStreamExt;
                let mut stream = response.bytes_stream().map_err(std::io::Error::other);
                let mut parser = SseEventParser::new();
                let mut buffer: Vec<u8> = Vec::new();

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(chunk) => {
                            buffer.extend_from_slice(&chunk);

                            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                                let mut line_bytes: Vec<u8> = buffer.drain(..=pos).collect();
                                if matches!(line_bytes.last(), Some(b'\n')) {
                                    line_bytes.pop();
                                }
                                if matches!(line_bytes.last(), Some(b'\r')) {
                                    line_bytes.pop();
                                }

                                let line_str = String::from_utf8_lossy(&line_bytes).to_string();
                                if let Some(event) = parser.feed_line(&line_str)
                                    && let Err(err) =
                                        Self::process_sse_event(&sse_url, event, &rpc_endpoint, &tx)
                                            .await
                                {
                                    error!("Failed to process SSE event: {}", err);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error in SSE stream: {}", e);
                            break;
                        }
                    }
                }

                if !buffer.is_empty() {
                    let line_str = String::from_utf8_lossy(&buffer).to_string();
                    if let Some(event) = parser.feed_line(&line_str)
                        && let Err(err) =
                            Self::process_sse_event(&sse_url, event, &rpc_endpoint, &tx).await
                    {
                        error!("Failed to process SSE event: {}", err);
                    }
                }
            }
        });
    }

    async fn process_sse_event(
        base_url: &Url,
        event: SseEvent,
        rpc_endpoint: &Arc<Mutex<Option<Url>>>,
        tx: &mpsc::Sender<RxJsonRpcMessage<RoleClient>>,
    ) -> Result<()> {
        if event.event_type.as_deref() == Some("endpoint") {
            let resolved = resolve_rpc_url(base_url, event.data.trim())?;
            let mut endpoint_lock = rpc_endpoint.lock().await;
            let update_needed = endpoint_lock
                .as_ref()
                .map(|current| current != &resolved)
                .unwrap_or(true);

            if update_needed {
                info!("RPC endpoint set to {}", resolved);
                *endpoint_lock = Some(resolved);
            }
            return Ok(());
        }

        if event.data.trim().is_empty() {
            return Ok(());
        }

        let message: RxJsonRpcMessage<RoleClient> = serde_json::from_str(&event.data)?;
        tx.send(message)
            .await
            .map_err(|e| McpHttpError::Custom(format!("Failed to deliver SSE message: {}", e)))?;

        Ok(())
    }

    async fn wait_for_rpc_endpoint(rpc_endpoint: Arc<Mutex<Option<Url>>>) -> Result<Url> {
        for _ in 0..RPC_ENDPOINT_WAIT_ATTEMPTS {
            if let Some(url) = rpc_endpoint.lock().await.clone() {
                return Ok(url);
            }
            tokio::time::sleep(Duration::from_millis(RPC_ENDPOINT_WAIT_DELAY_MS)).await;
        }

        Err(McpHttpError::MissingRpcEndpoint)
    }

    #[allow(dead_code)] // Will be used when HTTP client is integrated
    pub async fn ensure_authenticated(&self) -> Result<()> {
        let token_lock = self.token.lock().await;
        if token_lock.is_some() {
            // TODO: check for expiration and refresh
            return Ok(());
        }
        drop(token_lock);

        if let Some(oauth_client) = &self.oauth_client {
            info!("No token found, starting authentication flow.");
            let new_token = self.perform_oauth_flow(oauth_client).await?;
            let mut token_lock = self.token.lock().await;
            *token_lock = Some(new_token);
            info!("Authentication successful.");
            Ok(())
        } else {
            Err(McpHttpError::Auth(
                "OAuth client not configured, but authentication is required.".to_string(),
            ))
        }
    }

    #[allow(dead_code)] // Will be used when HTTP client is integrated
    async fn perform_oauth_flow(
        &self,
        oauth_client: &ConfiguredOAuthClient,
    ) -> Result<StoredToken> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let scopes = vec![
            "read:me",
            "read:confluence-content.summary",
            "read:confluence-space.summary",
            "read:confluence-props",
            "read:confluence-content.all",
            "read:confluence-user",
            "read:jira-user",
            "read:jira-work",
            "write:jira-work",
            "read:confluence-content.permission",
            "offline_access",
        ]
        .into_iter()
        .map(|s| Scope::new(s.to_string()));

        let (auth_url, csrf_token) = oauth_client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(pkce_challenge)
            .add_scopes(scopes)
            .url();

        info!("Please open this URL in your browser to authenticate:");
        info!("{}", auth_url);

        if webbrowser::open(auth_url.as_str()).is_err() {
            error!(
                "Failed to open web browser automatically. Please copy the URL and open it manually."
            );
        }

        let (code, state) = self.listen_for_callback_async().await?;

        if state.secret() != csrf_token.secret() {
            return Err(McpHttpError::Auth("CSRF token mismatch".to_string()));
        }

        let http_client = reqwest::Client::new();
        let token_result = oauth_client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(&http_client)
            .await
            .map_err(|e| McpHttpError::OAuth2(format!("{:?}", e)))?;

        let stored_token = StoredToken {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result
                .refresh_token()
                .map(|rt| rt.secret().to_string()),
            expires_in: token_result.expires_in().map(|d| d.as_secs()),
            scopes: token_result
                .scopes()
                .map(|s| s.iter().map(|sc| sc.to_string()).collect()),
        };

        save_token(&stored_token)?;

        Ok(stored_token)
    }

    #[allow(dead_code)] // Will be used when HTTP client is integrated
    async fn listen_for_callback_async(&self) -> Result<(String, CsrfToken)> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
        info!("Listening on http://127.0.0.1:8080 for OAuth callback.");

        let (mut stream, _) = listener.accept().await?;

        let (reader, mut writer) = tokio::io::split(&mut stream);
        let mut reader = tokio::io::BufReader::new(reader);

        let mut request_line = String::new();
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut request_line).await?;

        let redirect_url = request_line.split_whitespace().nth(1).unwrap_or("/");
        let url = Url::parse(&("http://localhost".to_string() + redirect_url))?;

        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.into_owned())
            .ok_or_else(|| McpHttpError::Auth("Missing auth code in callback".to_string()))?;

        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| CsrfToken::new(value.into_owned()))
            .ok_or_else(|| McpHttpError::Auth("Missing state in callback".to_string()))?;

        let message = "Authentication successful! You can close this tab.";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            message.len(),
            message
        );
        use tokio::io::AsyncWriteExt;
        writer.write_all(response.as_bytes()).await?;
        writer.flush().await?;

        Ok((code, state))
    }
}

impl Transport<RoleClient> for HttpMcpTransport {
    type Error = McpHttpError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl std::future::Future<Output = std::result::Result<(), Self::Error>> + Send + 'static
    {
        let client = self.client.clone();
        let token = self.token.clone();
        let rpc_endpoint = self.rpc_endpoint.clone();

        async move {
            let rpc_url = Self::wait_for_rpc_endpoint(rpc_endpoint).await?;

            // Ensure authenticated
            let token_lock = token.lock().await;
            let access_token = token_lock
                .as_ref()
                .map(|t| t.access_token.clone())
                .ok_or(McpHttpError::MissingAccessToken)?;
            drop(token_lock);

            let res = client
                .post(rpc_url)
                .bearer_auth(access_token)
                .json(&item)
                .send()
                .await?;

            if !res.status().is_success() {
                let status = res.status();
                let text = res.text().await.unwrap_or_default();
                let err_msg = format!("HTTP Error: {} - {}", status, text);
                error!("{}", err_msg);
                return Err(McpHttpError::Custom(err_msg));
            }

            Ok(())
        }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        let mut receiver = self.receiver.lock().await;
        receiver.recv().await
    }

    async fn close(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scopes: Option<Vec<String>>,
}

fn load_token() -> Result<Option<StoredToken>> {
    let path = token_file_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let token = serde_json::from_reader(reader)?;
    Ok(Some(token))
}

fn save_token(token: &StoredToken) -> Result<()> {
    let path = token_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, token)?;
    writer.flush()?;
    Ok(())
}

fn token_file_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os(TOKEN_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(env::temp_dir().join(TOKEN_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex as StdMutex, OnceLock};
    use tempfile::tempdir;

    fn token_env_guard() -> &'static StdMutex<()> {
        static GUARD: OnceLock<StdMutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| StdMutex::new(()))
    }

    #[tokio::test]
    async fn process_sse_endpoint_event_updates_rpc_url() {
        let base = Url::parse("https://example.com/sse").unwrap();
        let rpc_endpoint = Arc::new(Mutex::new(None));
        let (tx, mut rx) = mpsc::channel(1);
        let event = SseEvent {
            event_type: Some("endpoint".to_string()),
            data: "/mcp".to_string(),
        };

        HttpMcpTransport::process_sse_event(&base, event, &rpc_endpoint, &tx)
            .await
            .unwrap();

        let stored = rpc_endpoint.lock().await.clone().unwrap();
        assert_eq!(stored.as_str(), "https://example.com/mcp");
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn process_sse_event_forwards_jsonrpc_messages() {
        let base = Url::parse("https://example.com/sse").unwrap();
        let rpc_endpoint = Arc::new(Mutex::new(None));
        let (tx, mut rx) = mpsc::channel(1);
        let payload = r#"{"jsonrpc":"2.0","id":"42","result":{"message":"hi"}}"#;

        HttpMcpTransport::process_sse_event(
            &base,
            SseEvent {
                event_type: None,
                data: payload.to_string(),
            },
            &rpc_endpoint,
            &tx,
        )
        .await
        .unwrap();

        let received = rx.recv().await.unwrap();
        if !matches!(&received, RxJsonRpcMessage::<RoleClient>::Response(_)) {
            panic!("unexpected message: {:?}", received);
        }
        serde_json::to_string(&received).unwrap();
    }

    #[test]
    fn load_token_returns_none_when_override_missing() {
        let _guard = token_env_guard().lock().unwrap();
        let tmp = tempdir().unwrap();
        let token_path = tmp.path().join("custom_token.json");
        unsafe {
            env::set_var(TOKEN_PATH_ENV, token_path.to_str().unwrap());
        }

        let loaded = load_token().unwrap();
        assert!(loaded.is_none());

        unsafe {
            env::remove_var(TOKEN_PATH_ENV);
        }
    }

    #[test]
    fn save_token_round_trips_via_override_path() {
        let _guard = token_env_guard().lock().unwrap();
        let tmp = tempdir().unwrap();
        let token_path = tmp.path().join("custom_token.json");
        unsafe {
            env::set_var(TOKEN_PATH_ENV, token_path.to_str().unwrap());
        }

        let token = StoredToken {
            access_token: "abc123".to_string(),
            refresh_token: Some("ref-456".to_string()),
            expires_in: Some(3600),
            scopes: Some(vec!["scope1".to_string(), "scope2".to_string()]),
        };

        save_token(&token).unwrap();
        let loaded = load_token().unwrap().expect("token to exist");
        assert_eq!(loaded.access_token, token.access_token);
        assert_eq!(loaded.refresh_token, token.refresh_token);
        assert_eq!(loaded.expires_in, token.expires_in);
        assert_eq!(loaded.scopes, token.scopes);

        unsafe {
            env::remove_var(TOKEN_PATH_ENV);
        }
    }

    // === Additional client tests ===

    #[tokio::test]
    async fn process_sse_event_ignores_empty_data() {
        let base = Url::parse("https://example.com/sse").unwrap();
        let rpc_endpoint = Arc::new(Mutex::new(None));
        let (tx, mut rx) = mpsc::channel(1);

        HttpMcpTransport::process_sse_event(
            &base,
            SseEvent {
                event_type: None,
                data: "   ".to_string(), // Empty/whitespace only
            },
            &rpc_endpoint,
            &tx,
        )
        .await
        .unwrap();

        // Nothing should be sent
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn process_sse_event_endpoint_with_whitespace() {
        let base = Url::parse("https://example.com/sse").unwrap();
        let rpc_endpoint = Arc::new(Mutex::new(None));
        let (tx, _rx) = mpsc::channel(1);

        HttpMcpTransport::process_sse_event(
            &base,
            SseEvent {
                event_type: Some("endpoint".to_string()),
                data: "  /mcp  \n".to_string(), // Whitespace trimmed
            },
            &rpc_endpoint,
            &tx,
        )
        .await
        .unwrap();

        let stored = rpc_endpoint.lock().await.clone().unwrap();
        assert_eq!(stored.as_str(), "https://example.com/mcp");
    }

    #[tokio::test]
    async fn process_sse_event_invalid_json_fails() {
        let base = Url::parse("https://example.com/sse").unwrap();
        let rpc_endpoint = Arc::new(Mutex::new(None));
        let (tx, _rx) = mpsc::channel(1);

        let result = HttpMcpTransport::process_sse_event(
            &base,
            SseEvent {
                event_type: None,
                data: "not valid json".to_string(),
            },
            &rpc_endpoint,
            &tx,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn process_sse_endpoint_only_updates_when_different() {
        let base = Url::parse("https://example.com/sse").unwrap();
        let rpc_endpoint = Arc::new(Mutex::new(Some(
            Url::parse("https://example.com/mcp").unwrap(),
        )));
        let (tx, _rx) = mpsc::channel(1);

        // Same endpoint - should not log update
        HttpMcpTransport::process_sse_event(
            &base,
            SseEvent {
                event_type: Some("endpoint".to_string()),
                data: "/mcp".to_string(),
            },
            &rpc_endpoint,
            &tx,
        )
        .await
        .unwrap();

        // Endpoint should still be the same
        let stored = rpc_endpoint.lock().await.clone().unwrap();
        assert_eq!(stored.as_str(), "https://example.com/mcp");
    }

    #[tokio::test]
    async fn wait_for_rpc_endpoint_returns_immediately_when_set() {
        let rpc_endpoint = Arc::new(Mutex::new(Some(
            Url::parse("https://example.com/mcp").unwrap(),
        )));

        let result = HttpMcpTransport::wait_for_rpc_endpoint(rpc_endpoint).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "https://example.com/mcp");
    }

    #[test]
    fn token_file_path_uses_env_override() {
        let _guard = token_env_guard().lock().unwrap();
        let custom_path = "/custom/path/token.json";
        unsafe {
            env::set_var(TOKEN_PATH_ENV, custom_path);
        }

        let path = token_file_path().unwrap();
        assert_eq!(path.to_str().unwrap(), custom_path);

        unsafe {
            env::remove_var(TOKEN_PATH_ENV);
        }
    }

    #[test]
    fn token_file_path_uses_temp_dir_default() {
        let _guard = token_env_guard().lock().unwrap();
        unsafe {
            env::remove_var(TOKEN_PATH_ENV);
        }

        let path = token_file_path().unwrap();
        assert!(path.ends_with(TOKEN_FILE_NAME));
        assert!(path.starts_with(env::temp_dir()));
    }

    #[test]
    fn save_token_creates_parent_directories() {
        let _guard = token_env_guard().lock().unwrap();
        let tmp = tempdir().unwrap();
        let token_path = tmp.path().join("nested/deep/token.json");
        unsafe {
            env::set_var(TOKEN_PATH_ENV, token_path.to_str().unwrap());
        }

        let token = StoredToken {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_in: None,
            scopes: None,
        };

        save_token(&token).unwrap();
        assert!(token_path.exists());

        unsafe {
            env::remove_var(TOKEN_PATH_ENV);
        }
    }

    #[test]
    fn stored_token_minimal_fields() {
        let token = StoredToken {
            access_token: "test_token".to_string(),
            refresh_token: None,
            expires_in: None,
            scopes: None,
        };

        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("test_token"));

        let parsed: StoredToken = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "test_token");
        assert!(parsed.refresh_token.is_none());
    }

    #[test]
    fn stored_token_debug_display() {
        let token = StoredToken {
            access_token: "secret".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_in: Some(3600),
            scopes: Some(vec!["scope1".to_string()]),
        };

        let debug = format!("{:?}", token);
        assert!(debug.contains("StoredToken"));
        assert!(debug.contains("secret")); // Note: in real impl might want to redact
    }

    #[test]
    fn stored_token_clone() {
        let token = StoredToken {
            access_token: "abc".to_string(),
            refresh_token: Some("ref".to_string()),
            expires_in: Some(1800),
            scopes: Some(vec!["s1".to_string(), "s2".to_string()]),
        };

        let cloned = token.clone();
        assert_eq!(cloned.access_token, token.access_token);
        assert_eq!(cloned.refresh_token, token.refresh_token);
        assert_eq!(cloned.expires_in, token.expires_in);
        assert_eq!(cloned.scopes, token.scopes);
    }
}
