use ahma_mcp::terminal_output::TerminalOutput;
use ahma_mcp::test_utils::test_utils::assert_formatted_json_contains;
use ahma_mcp::utils::logging::init_test_logging;
use serde_json::json;

#[test]
fn test_should_display_comprehensive() {
    init_test_logging();
    // Empty cases
    assert!(!TerminalOutput::should_display(""));
    assert!(!TerminalOutput::should_display("   "));
    assert!(!TerminalOutput::should_display("\n\n\n"));
    assert!(!TerminalOutput::should_display("\t\t\t"));
    assert!(!TerminalOutput::should_display("   \n\t  \r  "));

    // Content cases
    assert!(TerminalOutput::should_display("a"));
    assert!(TerminalOutput::should_display("some content"));
    assert!(TerminalOutput::should_display("  content  "));
    assert!(TerminalOutput::should_display("\n  content  \n"));
    assert!(TerminalOutput::should_display("0")); // Number as string
    assert!(TerminalOutput::should_display("false")); // Boolean as string
}

#[test]
fn test_format_content_json_pretty_printing() {
    init_test_logging();
    // Simple JSON object
    let json_input = r#"{"name":"test","version":"1.0.0"}"#;
    assert_formatted_json_contains(
        json_input,
        &["{\n", "  \"name\": \"test\"", "  \"version\": \"1.0.0\""],
    );

    // Nested JSON
    let nested_json = r#"{"user":{"id":123,"name":"Alice","settings":{"theme":"dark"}}}"#;
    assert_formatted_json_contains(
        nested_json,
        &[
            "\"user\": {",
            "    \"id\": 123",
            "    \"settings\": {",
            "      \"theme\": \"dark\"",
        ],
    );

    // JSON array
    let array_json = r#"[{"id":1,"name":"first"},{"id":2,"name":"second"}]"#;
    assert_formatted_json_contains(array_json, &["[\n", "  {\n", "    \"id\": 1"]);
}

#[test]
fn test_format_content_invalid_json() {
    init_test_logging();
    // Invalid JSON should be treated as regular string
    let invalid_json = r#"{"name": invalid}"#;
    let formatted = TerminalOutput::format_content(invalid_json);
    assert_eq!(formatted, r#"{"name": invalid}"#);

    // Partial JSON
    let partial = r#"{"incomplete":"#;
    let formatted = TerminalOutput::format_content(partial);
    assert_eq!(formatted, r#"{"incomplete":"#); // Returns as-is since it's not valid JSON
}

#[test]
fn test_format_content_string_cleanup() {
    init_test_logging();
    // Escaped newlines
    let input = "Line 1\\nLine 2\\nLine 3";
    let formatted = TerminalOutput::format_content(input);
    assert_eq!(formatted, "Line 1\nLine 2\nLine 3");

    // Escaped tabs
    let input = "Column1\\tColumn2\\tColumn3";
    let formatted = TerminalOutput::format_content(input);
    assert_eq!(formatted, "Column1\tColumn2\tColumn3");

    // Escaped quotes
    let input = "He said \\\"Hello World\\\"";
    let formatted = TerminalOutput::format_content(input);
    assert_eq!(formatted, "He said \"Hello World\"");

    // Mixed escapes
    let input = "Mixed:\\nNewline\\tTab\\\"Quote";
    let formatted = TerminalOutput::format_content(input);
    assert_eq!(formatted, "Mixed:\nNewline\tTab\"Quote");

    // Whitespace trimming
    let input = "  \n  content with spaces  \n  ";
    let formatted = TerminalOutput::format_content(input);
    assert_eq!(formatted, "content with spaces");
}

#[test]
fn test_format_content_edge_cases() {
    init_test_logging();
    // Empty string
    let formatted = TerminalOutput::format_content("");
    assert_eq!(formatted, "");

    // Only whitespace
    let formatted = TerminalOutput::format_content("   \n\t  ");
    assert_eq!(formatted, "");

    // JSON null
    let formatted = TerminalOutput::format_content("null");
    assert_eq!(formatted, "null");

    // JSON boolean
    let formatted = TerminalOutput::format_content("true");
    assert_eq!(formatted, "true");

    // JSON number
    let formatted = TerminalOutput::format_content("42");
    assert_eq!(formatted, "42");

    // JSON string value
    let formatted = TerminalOutput::format_content("\"hello world\"");
    assert_eq!(formatted, "\"hello world\"");
}

#[test]
fn test_format_content_complex_json() {
    init_test_logging();
    // Real-world-like JSON structure
    let complex_json = json!({
        "status": "success",
        "data": {
            "items": [
                {"id": 1, "active": true, "metadata": null},
                {"id": 2, "active": false, "metadata": {"tags": ["urgent", "review"]}}
            ],
            "total": 2,
            "pagination": {
                "page": 1,
                "limit": 10,
                "has_more": false
            }
        },
        "timestamp": "2024-01-15T10:30:00Z"
    });

    let json_string = serde_json::to_string(&complex_json).unwrap();
    let formatted = TerminalOutput::format_content(&json_string);

    // Should be pretty printed
    assert!(formatted.contains("{\n"));
    assert!(formatted.contains("  \"status\": \"success\""));
    assert!(formatted.contains("  \"data\": {"));
    assert!(formatted.contains("    \"items\": ["));
    assert!(formatted.contains("      {"));
    assert!(formatted.contains("        \"id\": 1"));
    assert!(formatted.contains("        \"metadata\": null"));
    assert!(formatted.contains("          \"tags\": ["));
    assert!(formatted.contains("            \"urgent\""));
}

// Note: Testing display_result and display_await_results requires capturing stderr
// which is more complex. These functions primarily format and write to stderr,
// so their core logic is tested through format_content and should_display.

#[tokio::test]
async fn test_display_result_with_empty_content() {
    init_test_logging();
    // This should not panic and should handle empty content gracefully
    // The function returns early for empty content, so no output is produced
    TerminalOutput::display_result("test_op", "test_cmd", "test description", "").await;
    TerminalOutput::display_result("test_op", "test_cmd", "test description", "   \n\t  ").await;
}

#[tokio::test]
async fn test_display_await_results_with_empty_results() {
    init_test_logging();
    // Should handle empty results vector gracefully
    TerminalOutput::display_await_results(&[]).await;

    // Should handle vector with empty strings
    TerminalOutput::display_await_results(&[String::new(), "  ".to_string()]).await;
}

#[tokio::test]
async fn test_display_await_results_with_content() {
    init_test_logging();
    // Should handle multiple results
    let results = vec![
        r#"{"result": "first"}"#.to_string(),
        "Plain text result".to_string(),
        r#"{"result": "third", "status": "complete"}"#.to_string(),
    ];

    // This mainly tests that the function doesn't panic with valid input
    TerminalOutput::display_await_results(&results).await;
}
