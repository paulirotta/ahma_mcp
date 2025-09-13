//! # Standardized Constants and Templates
//!
//! T - If you'll need these results soon, start additional operations in parallel.- If you'll need these results soon, start additional operations in parallel.is module serves as a centralized repository for constants, particularly for strings
//! and templates that are presented to the AI agent. Centralizing these helps ensure
//! consistency in the guidance and instructions provided to the agent across different
//! parts of the application.
//!
//! ## Key Constants
//!
//! - **`ASYNC_ADDENDUM`**: A standard piece of text appended to the description of any tool
//!   that supports asynchronous execution. It instructs the agent on the correct protocol
//!   for handling async operations, emphasizing the use of `await` and `operation_ids`.
//!
//! - **`SYNC_ADDENDUM`**: A simpler addendum for tools that are synchronous or very fast,
//!   reminding the agent to use the provided MCP tools instead of raw shell commands.
//!
//! ## Guidance Templates
//!
//! These templates are used to generate dynamic, context-aware hints for the AI agent to
//! improve its performance and efficiency.
//!
//! - **`TOOL_HINT_TEMPLATE`**: A detailed message displayed when an asynchronous operation
//!   is initiated. It guides the agent on how to effectively multitask while waiting for
//!   the operation to complete, promoting non-blocking workflows.
//!
//! - **`CONCURRENCY_HINT_TEMPLATE`**: A hint triggered when the agent appears to be waiting
//!   prematurely for an operation to finish, encouraging it to perform other tasks in
//!   parallel to improve efficiency.
//!
//! - **`STATUS_POLLING_HINT_TEMPLATE`**: A message that detects when the agent is repeatedly
//!   checking the status of an operation. It suggests a more efficient pattern, such as
//!   using `await` with notifications, to avoid unnecessary polling.
//!
//! By using these standardized templates, the system can provide consistent, high-quality
//! feedback to the agent, helping it learn and adopt best practices for interacting with
//! the asynchronous tool server.

/// Standardized addendum for async-capable tools' descriptions used in documentation and help text.
pub const ASYNC_ADDENDUM: &str =
    "Always use ahma_mcp MCP tools; do not run any supported tools in the terminal";

/// Standardized addendum for synchronous or fast tools where async guidance is not emphasized.
pub const SYNC_ADDENDUM: &str = "Always use ahma_mcp MCP tools; do not run cargo in a terminal.";

/// Template for standardized tool-hint content displayed when async operations are started.
/// Placeholders:
/// - {operation_type}
/// - {operation_id}
pub const TOOL_HINT_TEMPLATE: &str = "\n\n### ASYNC AHMA OPERATION: {operation_type} (ID: {operation_id})\n\
1. The operation is running in the background — do not assume it is complete.\n\
2. What to do now (pick one):\n\
 - Update the plan with current achievements and list the next concrete steps.\n\
 - Do unrelated work (code, tests, documentation, user summary..) not blocked by this `{operation_type}`.\n\
 - If you’ll need these results soon, schedule a later `status` check instead of polling.\n\
 - If you have nothing else to do and need results to proceed, use `await`.\n\
3. Tips:\n\
 - **AVOID POLLING:** Do not repeatedly call `status` - this is inefficient and wastes resources.\n\
 - **Use `await` to block until operation ID(s) complete.**\n\
 - Batch actions: start multiple concurrent tools, then await for all IDs at once.\n\
Next: Continue useful work now. Use `await` when you actually need the results.\n\n";

/// Template used when detecting premature waits that harm concurrency.
/// Placeholders: {operation_id}, {gap_seconds}, {efficiency_percent}
pub const CONCURRENCY_HINT_TEMPLATE: &str = "CONCURRENCY HINT: You waited for '{operation_id}' after only {gap_seconds:.1}s (efficiency: {efficiency_percent:.0}%). \
        Consider performing other tasks while operations run in the background.";

/// Template for the status polling detection guidance.
/// Placeholders: {count}, {operation_id}
pub const STATUS_POLLING_HINT_TEMPLATE: &str = "**STATUS POLLING ANTI-PATTERN DETECTED:** You've called status {count} times for operation '{operation_id}'. \
  This is inefficient! Instead of repeatedly polling, use 'await' with the operation ID to get automatic completion notifications.\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::logging::init_test_logging;

    #[test]
    fn async_addendum_contains_key_guidance() {
        init_test_logging();
        assert!(ASYNC_ADDENDUM.contains("ahma_mcp"));
        assert!(ASYNC_ADDENDUM.contains("MCP tools"));
        assert!(ASYNC_ADDENDUM.contains("terminal")); // Key guidance to avoid direct terminal usage
    }

    #[test]
    fn templates_include_placeholders() {
        init_test_logging();
        assert!(TOOL_HINT_TEMPLATE.contains("{operation_type}"));
        assert!(TOOL_HINT_TEMPLATE.contains("{operation_id}"));
        assert!(CONCURRENCY_HINT_TEMPLATE.contains("{operation_id}"));
        assert!(STATUS_POLLING_HINT_TEMPLATE.contains("{operation_id}"));
    }
}
