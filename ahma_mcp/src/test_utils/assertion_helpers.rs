use std::future::Future;
use std::time::Duration;

/// Assert that an async operation completes within the expected time.
pub async fn assert_completes_within<T, Fut>(timeout: Duration, description: &str, fut: Fut) -> T
where
    Fut: Future<Output = T>,
{
    tokio::time::timeout(timeout, fut)
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Assertion failed: {} did not complete within {:?}",
                description, timeout
            )
        })
}

/// Assert that an async operation times out (does not complete).
///
/// Useful for testing that blocking operations properly block.
pub async fn assert_times_out<T, Fut>(timeout: Duration, description: &str, fut: Fut)
where
    Fut: Future<Output = T>,
{
    let result = tokio::time::timeout(timeout, fut).await;
    assert!(
        result.is_err(),
        "Expected {} to timeout, but it completed",
        description
    );
}

/// Assert that a condition becomes true within a timeout.
///
/// Polls the condition and provides clear failure messages.
pub async fn assert_eventually<F, Fut>(
    timeout: Duration,
    poll_interval: Duration,
    description: &str,
    mut condition: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if condition().await {
            return;
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!(
                "Assertion failed: '{}' did not become true within {:?}",
                description, timeout
            );
        }

        tokio::time::sleep(poll_interval.min(remaining)).await;
    }
}

/// Check if output contains any of the expected patterns
#[allow(dead_code)]
pub fn contains_any(output: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| output.contains(pattern))
}

/// Check if output contains all of the expected patterns
pub fn contains_all(output: &str, patterns: &[&str]) -> bool {
    patterns.iter().all(|pattern| output.contains(pattern))
}

/// Extract tool names from debug output
pub fn extract_tool_names(debug_output: &str) -> Vec<String> {
    let mut tool_names = Vec::new();
    for line in debug_output.lines() {
        if (line.contains("Loading tool:") || line.contains("Tool loaded:"))
            && let Some(name) = line.split(':').nth(1)
        {
            tool_names.push(name.trim().to_string());
        }
    }
    tool_names
}

/// Helper to assert that formatting a JSON string via TerminalOutput contains all expected substrings.
#[allow(dead_code)]
pub fn assert_formatted_json_contains(raw: &str, expected: &[&str]) {
    let formatted = crate::terminal_output::TerminalOutput::format_content(raw);
    for token in expected {
        assert!(
            formatted.contains(token),
            "Formatted content missing token: {token}"
        );
    }
}
