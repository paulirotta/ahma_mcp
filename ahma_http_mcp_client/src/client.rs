use crate::error::{McpHttpError, Result};
use futures::{Stream, StreamExt};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl, basic::BasicClient,
};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf, pin::Pin, sync::Arc};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, info};
use url::Url;

const TOKEN_FILE: &str = "mcp_http_token.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct StoredToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scopes: Option<Vec<String>>,
}

fn get_token_path() -> Result<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push(TOKEN_FILE);
    Ok(path)
}

fn save_token(token: &StoredToken) -> Result<()> {
    let path = get_token_path()?;
    let file = std::fs::File::create(path)?;
    serde_json::to_writer(file, token)?;
    Ok(())
}

fn load_token() -> Result<Option<StoredToken>> {
    let path = get_token_path()?;
    if path.exists() {
        let file = std::fs::File::open(path)?;
        let token = serde_json::from_reader(file)?;
        Ok(Some(token))
    } else {
        Ok(None)
    }
}

pub struct HttpMcpTransport {
    client: reqwest::Client,
    url: Url,
    token: Arc<Mutex<Option<StoredToken>>>,
    oauth_client: Option<BasicClient>,
    sender: mpsc::Sender<McpMessage>,
}

impl HttpMcpTransport {
    pub fn new(url: Url, client_id: Option<String>, client_secret: Option<String>) -> Result<Self> {
        let oauth_client =
            if let (Some(client_id), Some(client_secret)) = (client_id, client_secret) {
                Some(
                    BasicClient::new(
                        ClientId::new(client_id),
                        Some(ClientSecret::new(client_secret)),
                        AuthUrl::new("https://auth.atlassian.com/authorize".to_string())?,
                        Some(TokenUrl::new(
                            "https://auth.atlassian.com/oauth/token".to_string(),
                        )?),
                    )
                    .set_redirect_uri(RedirectUrl::new("http://localhost:8080".to_string())?),
                )
            } else {
                None
            };

        let (sender, _) = mpsc::channel(100);

        Ok(Self {
            client: reqwest::Client::new(),
            url,
            token: Arc::new(Mutex::new(load_token()?)),
            oauth_client,
            sender,
        })
    }

    async fn ensure_authenticated(&self) -> Result<()> {
        let mut token_lock = self.token.lock().await;
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

    async fn perform_oauth_flow(&self, oauth_client: &BasicClient) -> Result<StoredToken> {
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

        let token_result = oauth_client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
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

#[async_trait::async_trait]
impl McpTransport for HttpMcpTransport {
    async fn send(&self, msg: McpMessageWithSession) -> std::result::Result<(), McpError> {
        if let Err(e) = self.ensure_authenticated().await {
            error!("Authentication failed: {}", e);
            return Err(McpError::new_custom("Authentication", &e.to_string(), true));
        }

        let token = self.token.lock().await;
        let access_token = token
            .as_ref()
            .map(|t| t.access_token.clone())
            .ok_or_else(|| McpError::new_custom("Authentication", "Missing access token", true))?;
        drop(token);

        let res = self
            .client
            .post(self.url.clone())
            .bearer_auth(access_token)
            .json(&msg)
            .send()
            .await
            .map_err(|e| McpError::new_transport(&e.to_string(), false))?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            let err_msg = format!("HTTP Error: {} - {}", status, text);
            error!("{}", err_msg);
            return Err(McpError::new_transport(&err_msg, true));
        }

        Ok(())
    }

    fn messages(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = std::result::Result<McpMessage, McpError>> + Send>> {
        let url = self.url.clone();
        let token_arc = self.token.clone();
        let (tx, mut rx) = mpsc::channel(100);
        self.sender = tx.clone();

        tokio::spawn(async move {
            loop {
                let access_token = match token_arc.lock().await.as_ref() {
                    Some(t) => t.access_token.clone(),
                    None => {
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    }
                };

                let mut stream = match reqwest::Client::new()
                    .get(url.clone())
                    .bearer_auth(&access_token)
                    .send()
                    .await
                {
                    Ok(res) if res.status().is_success() => res.bytes_stream(),
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

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(chunk) => {
                            for line in chunk.split(|&b| b == b'\n') {
                                let line_str = String::from_utf8_lossy(line);
                                if line_str.starts_with("data: ") {
                                    let data = &line_str["data: ".len()..].trim();
                                    if !data.is_empty() {
                                        match serde_json::from_str::<rmcp::model::McpMessage>(data)
                                        {
                                            Ok(msg) => {
                                                if let Err(e) = tx.send(msg).await {
                                                    error!(
                                                        "Failed to send McpMessage to channel: {}",
                                                        e
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                error!("Failed to deserialize McpMessage: {}", e);
                                                debug!("Invalid data: {}", data);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error in SSE stream: {}", e);
                            break;
                        }
                    }
                }
            }
        });

        let stream = async_stream::stream! {
            while let Some(msg) = rx.recv().await {
                yield Ok(msg);
            }
        };

        Box::pin(stream)
    }
}
