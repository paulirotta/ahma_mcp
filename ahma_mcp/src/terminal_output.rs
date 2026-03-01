//! # Terminal Output Formatting Utilities
//!
//! This module provides a set of utilities for formatting and displaying command output
//! in a human-readable format on the terminal. A key design principle here is that all
//! human-readable output is written to `stderr`, while `stdout` is reserved exclusively
//! for the machine-readable MCP (Machine-Checked Protocol) JSON transport. This separation
//! is crucial for ensuring that the server can communicate with an MCP client correctly
//! while still providing useful diagnostic information to a human operator observing
//! the server's console.
//!
//! ## Core Components
//!
//! * **`TerminalOutput`**: A utility struct that encapsulates the formatting logic.
//!
//! ## Key Functions
//!
//! * **`display_result`**: Takes the details of a single operation (ID, command, description,
//!   and content) and formats them into a structured, easy-to-read block on `stderr`.
//!   It includes a header with the operation ID and metadata about the command.
//!
//! * **`display_await_results`**: Specifically designed to display the results of a `await`
//!   command, which can return the output of multiple operations at once. It formats
//!   each result individually and separates them with a clear visual divider.
//!
//! * **`format_content`**: This is the core formatting logic. It first attempts to parse
//!   the input content as JSON. If successful, it pretty-prints the JSON with indentation,
//!   making it much easier to read. If the content is not valid JSON, it performs basic
//!   string cleanup, such as converting escaped newlines (`\\n`) and tabs (`\\t`) into
//!   actual control characters.
//!
//! * **`should_display`**: A simple helper to check if a given string contains any
//!   non-whitespace content, preventing the display of empty or meaningless output.
//!
//! ## Design Philosophy
//!
//! The strict separation of `stdout` (for machines) and `stderr` (for humans) is a
//! fundamental design choice. It allows the `ahma_mcp` server to be used both as a
//! backend for an automated AI agent (which only cares about the JSON on `stdout`) and
//! as a standalone tool that can be debugged and monitored by a human developer (who
//! can watch the formatted output on `stderr`).

use serde_json::Value;
use tokio::io::{AsyncWriteExt, stderr};

/// Terminal output utility for formatting and displaying command results
pub struct TerminalOutput;

impl TerminalOutput {
    /// Write a formatted result to terminal with proper headers and formatting
    pub async fn display_result(id: &str, command: &str, description: &str, content: &str) {
        if content.trim().is_empty() {
            return; // Skip whitespace-only content
        }

        // IMPORTANT: Write human-readable diagnostics to stderr so stdout remains a pure MCP JSON transport.
        let mut stderr = stderr();
        let _ = stderr.write_all(b"\n").await;
        let _ = stderr
            .write_all(format!("=== {} ===\n", id.to_uppercase()).as_bytes())
            .await;
        let _ = stderr
            .write_all(format!("Command: {}\n", command).as_bytes())
            .await;
        let _ = stderr
            .write_all(format!("Description: {}\n", description).as_bytes())
            .await;
        let _ = stderr.write_all(b"\n").await;

        // Format the content
        let formatted_content = Self::format_content(content);
        let _ = stderr.write_all(formatted_content.as_bytes()).await;
        let _ = stderr.write_all(b"\n").await;
        let _ = stderr.flush().await;
    }

    /// Format content with proper JSON pretty-printing and newline handling
    pub fn format_content(content: &str) -> String {
        // Try to parse as JSON first
        if let Ok(json_value) = serde_json::from_str::<Value>(content) {
            // Pretty print JSON with 2-space indentation
            if let Ok(pretty_json) = serde_json::to_string_pretty(&json_value) {
                return pretty_json;
            }
        }

        // If not JSON, handle newlines and clean up formatting
        content
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .trim()
            .to_string()
    }

    /// Display multiple operation results from await command
    pub async fn display_await_results(results: &[String]) {
        if results.is_empty() {
            return;
        }

        let mut stderr = stderr();
        let _ = stderr.write_all(b"\n").await;
        let _ = stderr
            .write_all(b"===============================================================\n")
            .await;
        let _ = stderr
            .write_all(b"                    OPERATION RESULTS                         \n")
            .await;
        let _ = stderr
            .write_all(b"===============================================================\n")
            .await;
        let _ = stderr.write_all(b"\n").await;

        for (i, result) in results.iter().enumerate() {
            if i > 0 {
                let _ = stderr
                    .write_all(
                        b"\n---------------------------------------------------------------\n\n",
                    )
                    .await;
            }

            let formatted = Self::format_content(result);
            let _ = stderr.write_all(formatted.as_bytes()).await;
            let _ = stderr.write_all(b"\n").await;
        }

        let _ = stderr
            .write_all(b"===============================================================\n")
            .await;
        let _ = stderr.flush().await;
    }

    /// Check if content should be displayed (not just whitespace)
    pub fn should_display(content: &str) -> bool {
        !content.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::logging::init_test_logging;

    #[test]
    fn test_should_display() {
        init_test_logging();
        assert!(!TerminalOutput::should_display(""));
        assert!(!TerminalOutput::should_display("   \n\t  "));
        assert!(TerminalOutput::should_display("some content"));
        assert!(TerminalOutput::should_display("  content  "));
    }

    #[test]
    fn test_format_content() {
        init_test_logging();
        // Test JSON formatting
        let json_input = r#"{"name":"test","version":"1.0.0"}"#;
        let formatted = TerminalOutput::format_content(json_input);
        assert!(formatted.contains("{\n"));
        assert!(formatted.contains("  \"name\": \"test\""));

        // Test regular string formatting
        let string_input = "Hello\\nWorld\\tTab";
        let formatted = TerminalOutput::format_content(string_input);
        assert_eq!(formatted, "Hello\nWorld\tTab");
    }
}
