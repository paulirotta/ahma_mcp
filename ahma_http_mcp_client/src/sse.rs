use crate::error::McpHttpError;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event_type: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseEventParser {
    current_event: Option<String>,
    data: String,
}

impl SseEventParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed_line(&mut self, raw_line: &str) -> Option<SseEvent> {
        let line = raw_line.trim_end_matches(['\r', '\n']);

        if line.is_empty() {
            if self.current_event.is_none() && self.data.is_empty() {
                return None;
            }

            let event = SseEvent {
                event_type: self.current_event.clone(),
                data: std::mem::take(&mut self.data),
            };
            self.current_event = None;
            return Some(event);
        }

        if line.starts_with(":") {
            return None;
        }

        if let Some(stripped) = line.strip_prefix("event:") {
            let value = stripped.trim_start();
            self.current_event = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
            return None;
        }

        if let Some(stripped) = line.strip_prefix("data:") {
            let value = stripped.trim_start();
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(value);
            return None;
        }

        None
    }
}

pub fn resolve_rpc_url(sse_url: &Url, endpoint: &str) -> Result<Url, McpHttpError> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return Err(McpHttpError::Custom(
            "Empty RPC endpoint announced".to_string(),
        ));
    }

    match Url::parse(trimmed) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            sse_url.join(trimmed).map_err(McpHttpError::from)
        }
        Err(err) => Err(McpHttpError::from(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_url() -> Url {
        Url::parse("https://example.com/v1/sse").unwrap()
    }

    #[test]
    fn resolves_relative_endpoint() {
        let url = resolve_rpc_url(&base_url(), "mcp").expect("url");
        assert_eq!(url.as_str(), "https://example.com/v1/mcp");
    }

    #[test]
    fn resolves_absolute_path_endpoint() {
        let url = resolve_rpc_url(&base_url(), "/bridge/mcp").expect("url");
        assert_eq!(url.as_str(), "https://example.com/bridge/mcp");
    }

    #[test]
    fn resolves_full_url_endpoint() {
        let url = resolve_rpc_url(&base_url(), "https://api.atlassian.com/mcp").expect("url");
        assert_eq!(url.as_str(), "https://api.atlassian.com/mcp");
    }

    #[test]
    fn parses_endpoint_event() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("event: endpoint").is_none());
        assert!(parser.feed_line("data: /mcp").is_none());
        let event = parser.feed_line("").expect("event");
        assert_eq!(event.event_type.as_deref(), Some("endpoint"));
        assert_eq!(event.data, "/mcp");
    }
}
