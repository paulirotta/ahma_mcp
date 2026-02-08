use crate::operation_monitor::Operation;
use rmcp::model::Content;
use serde_json::{Map, Value};

/// Parses a comma-separated string value from JSON args into a list of trimmed, non-empty strings.
pub fn parse_comma_separated_filter(args: &Map<String, Value>, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| {
            s.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Serializes operations to Content text entries, logging errors.
pub fn serialize_operations_to_content(operations: &[Operation]) -> Vec<Content> {
    operations
        .iter()
        .filter_map(|op| match serde_json::to_string_pretty(op) {
            Ok(s) => Some(Content::text(s)),
            Err(e) => {
                tracing::error!("Serialization error: {}", e);
                None
            }
        })
        .collect()
}

/// Checks whether an operation matches the given tool name prefixes and optional operation ID.
pub fn operation_matches_filters(
    op: &Operation,
    tool_filters: &[String],
    operation_id: Option<&str>,
) -> bool {
    let matches_filter =
        tool_filters.is_empty() || tool_filters.iter().any(|tn| op.tool_name.starts_with(tn));
    let matches_id = operation_id.is_none_or(|id| op.id == id);
    matches_filter && matches_id
}

pub fn parse_tool_filters(args: &Map<String, Value>) -> Vec<String> {
    parse_comma_separated_filter(args, "tools")
}

pub fn parse_operation_id(args: &Map<String, Value>) -> Option<String> {
    args.get("operation_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
