//! Test coverage for terminal output formatting edge cases
//!
//! This test module targets untested paths in terminal output formatting
//! to improve code coverage.

use ahma_core::terminal_output::TerminalOutput;
use ahma_core::utils::logging::init_test_logging;

#[tokio::test]
async fn test_display_result_with_empty_content() {
    init_test_logging();

    // Test with completely empty content - should return early
    TerminalOutput::display_result("empty_test", "test command", "test description", "").await;

    // Test with whitespace-only content - should return early
    TerminalOutput::display_result(
        "whitespace_test",
        "test command",
        "test description",
        "   \n\t  \r\n  ",
    )
    .await;

    // These calls should complete without writing anything to stderr
    // (testing the early return path in display_result)
}

#[tokio::test]
async fn test_display_result_with_actual_content() {
    init_test_logging();

    // Test with actual content to exercise the full formatting path
    TerminalOutput::display_result(
        "content_test",
        "echo hello",
        "Simple echo command",
        "Hello, World!\nThis is a test.",
    )
    .await;

    // Test with JSON content
    TerminalOutput::display_result(
        "json_test",
        "cargo metadata",
        "Get cargo metadata",
        r#"{"name": "test", "version": "1.0.0", "dependencies": []}"#,
    )
    .await;
}

#[tokio::test]
async fn test_format_content_edge_cases() {
    init_test_logging();

    // Test empty string
    let result = TerminalOutput::format_content("");
    assert_eq!(result, "");

    // Test whitespace-only string
    let result = TerminalOutput::format_content("   \n\t  ");
    assert_eq!(result, "");

    // Test malformed JSON
    let malformed_json = r#"{"incomplete": "json""#;
    let result = TerminalOutput::format_content(malformed_json);
    assert_eq!(result, r#"{"incomplete": "json""#);

    // Test valid JSON with complex escaping
    let complex_json =
        r#"{"path": "C:\\Users\\test", "message": "Hello\nWorld", "quoted": "He said \"Hello\""}"#;
    let result = TerminalOutput::format_content(complex_json);
    assert!(result.contains("{\n"));
    assert!(result.contains("  \"path\""));

    // Test string with escaped characters
    let escaped_string = "Line1\\nLine2\\tTabbed\\\"Quoted\\\"";
    let result = TerminalOutput::format_content(escaped_string);
    assert_eq!(result, "Line1\nLine2\tTabbed\"Quoted\"");

    // Test string with multiple types of escapes
    let multi_escape = "Path: C:\\\\folder\\\\file.txt\\nNew line\\tTab\\\"Quote\\\"";
    let result = TerminalOutput::format_content(multi_escape);
    assert_eq!(
        result,
        "Path: C:\\\\folder\\\\file.txt\nNew line\tTab\"Quote\""
    );

    // Test already unescaped string
    let normal_string = "This is a normal string with spaces";
    let result = TerminalOutput::format_content(normal_string);
    assert_eq!(result, "This is a normal string with spaces");
}

#[tokio::test]
async fn test_display_await_results_edge_cases() {
    init_test_logging();

    // Test with empty results array
    let empty_results: Vec<String> = vec![];
    TerminalOutput::display_await_results(&empty_results).await;

    // Test with single result
    let single_result = vec!["Single result content".to_string()];
    TerminalOutput::display_await_results(&single_result).await;

    // Test with multiple results including JSON and plain text
    let multiple_results = vec![
        r#"{"operation": "build", "status": "success"}"#.to_string(),
        "Plain text output\nwith multiple lines".to_string(),
        "".to_string(), // Empty result
        r#"{"operation": "test", "status": "failed", "error": "Assertion failed"}"#.to_string(),
    ];
    TerminalOutput::display_await_results(&multiple_results).await;

    // Test with results containing escaped characters
    let escaped_results = vec![
        "Result with\\nescaped\\nlines".to_string(),
        r#"{"message": "Error\\noccurred", "path": "C:\\\\temp\\\\file.txt"}"#.to_string(),
    ];
    TerminalOutput::display_await_results(&escaped_results).await;
}

#[tokio::test]
async fn test_should_display_comprehensive() {
    init_test_logging();

    // Test various empty/whitespace cases
    assert!(!TerminalOutput::should_display(""));
    assert!(!TerminalOutput::should_display("   "));
    assert!(!TerminalOutput::should_display("\n"));
    assert!(!TerminalOutput::should_display("\t"));
    assert!(!TerminalOutput::should_display("\r"));
    assert!(!TerminalOutput::should_display("  \n\t\r  "));
    assert!(!TerminalOutput::should_display("\n\n\n"));

    // Test cases that should display
    assert!(TerminalOutput::should_display("a"));
    assert!(TerminalOutput::should_display("content"));
    assert!(TerminalOutput::should_display("  content  "));
    assert!(TerminalOutput::should_display("\n  content  \n"));
    assert!(TerminalOutput::should_display("0")); // Zero character
    assert!(TerminalOutput::should_display("false")); // Boolean false as string

    // Test edge cases with special characters
    assert!(TerminalOutput::should_display(".")); // Single period
    assert!(TerminalOutput::should_display("!")); // Exclamation
    assert!(TerminalOutput::should_display("@#$%")); // Special characters
    assert!(TerminalOutput::should_display("🚀")); // Unicode emoji
}

#[test]
fn test_format_content_json_parsing_edge_cases() {
    init_test_logging();

    // Test deeply nested JSON
    let nested_json = r#"{"level1": {"level2": {"level3": {"data": "deep"}}}}"#;
    let result = TerminalOutput::format_content(nested_json);
    assert!(result.contains("{\n"));
    assert!(result.contains("  \"level1\""));

    // Test JSON array
    let json_array = r#"[{"name": "item1"}, {"name": "item2"}]"#;
    let result = TerminalOutput::format_content(json_array);
    assert!(result.contains("[\n"));

    // Test JSON with null values
    let json_with_null = r#"{"value": null, "empty": "", "number": 0}"#;
    let result = TerminalOutput::format_content(json_with_null);
    assert!(result.contains("null"));
    assert!(result.contains("\"\""));

    // Test JSON with boolean values
    let json_with_bool = r#"{"success": true, "failed": false}"#;
    let result = TerminalOutput::format_content(json_with_bool);
    assert!(result.contains("true"));
    assert!(result.contains("false"));

    // Test malformed JSON variants
    let malformed_variants = vec![
        r#"{"incomplete""#,
        r#"{"missing_value":}"#,
        r#"{"trailing_comma": "value",}"#,
        r#"{unquoted_key: "value"}"#,
        r#"{"single_quotes": 'value'}"#,
    ];

    for malformed in malformed_variants {
        let result = TerminalOutput::format_content(malformed);
        // Should return the original string when JSON parsing fails
        assert_eq!(result, malformed);
    }
}
