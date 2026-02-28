//! Type definitions for the adapter module.

use serde::{Deserialize, Serialize};
use serde_json::Map;

/// Execution strategy for tool calls.
///
/// Determines whether a tool is executed immediately on the current task or
/// dispatched asynchronously with progress updates sent to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Run the tool inline and return the final output to the caller.
    Synchronous,
    /// Run the tool asynchronously and push results via callbacks.
    AsyncResultPush,
}

/// Options for configuring asynchronous execution.
pub struct AsyncExecOptions<'a> {
    /// Optional pre-defined operation ID to use; if None, a new one is generated.
    pub operation_id: Option<String>,
    /// Structured arguments for the command (positional and flags derived internally).
    pub args: Option<Map<String, serde_json::Value>>,
    /// Timeout in seconds for the command; falls back to shell pool default if None.
    pub timeout: Option<u64>,
    /// Optional callback to receive progress and final result notifications.
    pub callback: Option<Box<dyn crate::callback_system::CallbackSender>>,
    /// Subcommand configuration for handling positional arguments and aliases.
    pub subcommand_config: Option<&'a crate::config::SubcommandConfig>,
    /// Optional log monitor configuration for live stderr/stdout monitoring.
    /// When set, the adapter streams output line-by-line instead of collecting it all at once.
    pub log_monitor_config: Option<crate::log_monitor::LogMonitorConfig>,
}
