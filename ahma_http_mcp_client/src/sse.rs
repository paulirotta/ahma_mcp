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

    // === Additional SSE Parser Tests ===

    #[test]
    fn parser_ignores_comment_lines() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line(": this is a comment").is_none());
        assert!(parser.feed_line(":another comment").is_none());
        // Parser state should be empty
        assert!(parser.feed_line("").is_none());
    }

    #[test]
    fn parser_handles_multiline_data() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("data: line1").is_none());
        assert!(parser.feed_line("data: line2").is_none());
        assert!(parser.feed_line("data: line3").is_none());
        let event = parser.feed_line("").expect("event");
        assert!(event.event_type.is_none());
        assert_eq!(event.data, "line1\nline2\nline3");
    }

    #[test]
    fn parser_handles_event_without_data() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("event: ping").is_none());
        let event = parser.feed_line("").expect("event");
        assert_eq!(event.event_type.as_deref(), Some("ping"));
        assert!(event.data.is_empty());
    }

    #[test]
    fn parser_handles_data_without_event() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("data: just data").is_none());
        let event = parser.feed_line("").expect("event");
        assert!(event.event_type.is_none());
        assert_eq!(event.data, "just data");
    }

    #[test]
    fn parser_handles_empty_event_type() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("event:").is_none());
        assert!(parser.feed_line("data: test").is_none());
        let event = parser.feed_line("").expect("event");
        assert!(event.event_type.is_none()); // Empty event type should be None
        assert_eq!(event.data, "test");
    }

    #[test]
    fn parser_handles_crlf_line_endings() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("data: test\r\n").is_none());
        let event = parser.feed_line("\r\n").expect("event");
        assert_eq!(event.data, "test");
    }

    #[test]
    fn parser_ignores_unrecognized_fields() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("id: 123").is_none()); // id is not handled
        assert!(parser.feed_line("retry: 5000").is_none()); // retry is not handled
        assert!(parser.feed_line("data: actual data").is_none());
        let event = parser.feed_line("").expect("event");
        assert_eq!(event.data, "actual data");
    }

    #[test]
    fn parser_resets_after_event() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("event: first").is_none());
        assert!(parser.feed_line("data: data1").is_none());
        let event1 = parser.feed_line("").expect("event1");
        assert_eq!(event1.event_type.as_deref(), Some("first"));
        assert_eq!(event1.data, "data1");

        // Second event should start fresh
        assert!(parser.feed_line("event: second").is_none());
        assert!(parser.feed_line("data: data2").is_none());
        let event2 = parser.feed_line("").expect("event2");
        assert_eq!(event2.event_type.as_deref(), Some("second"));
        assert_eq!(event2.data, "data2");
    }

    #[test]
    fn parser_handles_whitespace_in_data() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("data:   spaced  data  ").is_none());
        let event = parser.feed_line("").expect("event");
        // Leading space after colon is trimmed, trailing is kept
        assert_eq!(event.data, "spaced  data  ");
    }

    #[test]
    fn parser_consecutive_empty_lines_no_duplicate_events() {
        let mut parser = SseEventParser::new();
        assert!(parser.feed_line("data: test").is_none());
        let event = parser.feed_line("").expect("event");
        assert_eq!(event.data, "test");

        // Subsequent empty lines should not produce events
        assert!(parser.feed_line("").is_none());
        assert!(parser.feed_line("").is_none());
    }

    // === resolve_rpc_url edge cases ===

    #[test]
    fn resolve_rpc_url_empty_endpoint_fails() {
        let result = resolve_rpc_url(&base_url(), "");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Empty RPC endpoint"));
    }

    #[test]
    fn resolve_rpc_url_whitespace_only_fails() {
        let result = resolve_rpc_url(&base_url(), "   ");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_rpc_url_trims_whitespace() {
        let url = resolve_rpc_url(&base_url(), "  /mcp  ").expect("url");
        assert_eq!(url.as_str(), "https://example.com/mcp");
    }

    #[test]
    fn resolve_rpc_url_with_query_params() {
        let url = resolve_rpc_url(&base_url(), "/mcp?session=123").expect("url");
        assert_eq!(url.as_str(), "https://example.com/mcp?session=123");
    }

    #[test]
    fn resolve_rpc_url_different_port() {
        let base = Url::parse("https://example.com:8443/v1/sse").unwrap();
        let url = resolve_rpc_url(&base, "/mcp").expect("url");
        assert_eq!(url.as_str(), "https://example.com:8443/mcp");
    }

    #[test]
    fn resolve_rpc_url_full_url_different_host() {
        let url = resolve_rpc_url(&base_url(), "https://other.example.com/api").expect("url");
        assert_eq!(url.as_str(), "https://other.example.com/api");
    }

    #[test]
    fn resolve_rpc_url_preserves_path_segments() {
        let base = Url::parse("https://example.com/api/v1/sse").unwrap();
        let url = resolve_rpc_url(&base, "rpc").expect("url");
        // "rpc" relative to "sse" replaces "sse"
        assert_eq!(url.as_str(), "https://example.com/api/v1/rpc");
    }
}
