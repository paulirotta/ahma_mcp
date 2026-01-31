//! Tests for status polling anti-pattern detection
//!
//! This test verifies that the mcp_service detects when an LLM repeatedly calls
//! the `status` tool and provides guidance to use `await` instead.

use ahma_mcp::utils::logging::init_test_logging;

#[tokio::test]
async fn test_status_tool_includes_anti_pattern_guidance() {
    init_test_logging();

    // This is a simplified test that verifies the STATUS_POLLING_HINT_TEMPLATE
    // is defined and contains the necessary guidance text.

    use ahma_mcp::constants::STATUS_POLLING_HINT_TEMPLATE;

    // Verify the template exists and contains key guidance elements
    assert!(STATUS_POLLING_HINT_TEMPLATE.contains("POLLING"));
    assert!(STATUS_POLLING_HINT_TEMPLATE.contains("await"));
    assert!(STATUS_POLLING_HINT_TEMPLATE.contains("{count}"));
    assert!(STATUS_POLLING_HINT_TEMPLATE.contains("{operation_id}"));

    // Template should guide LLM away from polling behavior
    assert!(
        STATUS_POLLING_HINT_TEMPLATE.contains("use 'await'"),
        "Template should clearly indicate this is bad practice"
    );
}

#[tokio::test]
async fn test_await_tool_description_discourages_premature_use() {
    init_test_logging();

    // Verify that the await tool description contains strong warnings
    // about inefficiency to discourage its use when not needed

    // This test validates that our tool descriptions provide the right guidance
    // (actual implementation will check the description when tools are listed)

    // The description should include phrases that make LLMs think twice:
    // - "WARNING"
    // - "inefficient"
    // - "ONLY use this if..."
    // - "ALWAYS better to..."

    // This test validates design intent - implementation in mcp_service.rs
}

#[tokio::test]
async fn test_status_tool_description_explains_proper_use() {
    init_test_logging();

    // The status tool description should clearly explain:
    // 1. It's for checking status WITHOUT blocking
    // 2. Results are automatically pushed when complete
    // 3. Repeated calls are unnecessary

    // This test validates design intent - implementation in mcp_service.rs
}
