use crate::AhmaMcpService;
use rmcp::model::{CallToolResult, Content, ErrorData as McpError};
use serde_json::{Map, Value};

impl AhmaMcpService {
    /// Handles the 'cancel' tool call.
    pub async fn handle_cancel(
        &self,
        args: Map<String, Value>,
    ) -> Result<CallToolResult, McpError> {
        let operation_id = args
            .get("operation_id")
            .ok_or_else(|| {
                McpError::invalid_params(
                    "operation_id parameter is required".to_string(),
                    Some(serde_json::json!({ "missing_param": "operation_id" })),
                )
            })?
            .as_str()
            .ok_or_else(|| {
                McpError::invalid_params(
                    "operation_id must be a string".to_string(),
                    Some(serde_json::json!({ "operation_id": args.get("operation_id") })),
                )
            })?
            .to_string();

        // Optional cancellation reason to aid debugging
        let reason: Option<String> = args
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Attempt to cancel the operation
        let cancelled = self
            .operation_monitor
            .cancel_operation_with_reason(&operation_id, reason.clone())
            .await;

        let result_message = if cancelled {
            let why = reason
                .as_deref()
                .unwrap_or("No reason provided (default: user-initiated)");
            format!(
                "OK Operation '{}' has been cancelled successfully.\nString: reason='{}'.\nHint: Consider restarting the operation if needed.",
                operation_id, why
            )
        } else {
            // Check if operation exists but is already terminal
            if let Some(operation) = self.operation_monitor.get_operation(&operation_id).await {
                format!(
                    "WARNING Operation '{}' is already {} and cannot be cancelled.",
                    operation_id,
                    match operation.state {
                        crate::operation_monitor::OperationStatus::Completed => "completed",
                        crate::operation_monitor::OperationStatus::Failed => "failed",
                        crate::operation_monitor::OperationStatus::Cancelled => "cancelled",
                        crate::operation_monitor::OperationStatus::TimedOut => "timed out",
                        _ => "in a terminal state",
                    }
                )
            } else {
                format!(
                    "FAIL Operation '{}' not found. It may have already completed or never existed.",
                    operation_id
                )
            }
        };

        // Add a machine-parseable suggestion block to encourage restart via tool hint
        let suggestion = serde_json::json!({
            "tool_hint": {
                "suggested_tool": "status",
                "reason": "Operation cancelled; check status and consider restarting",
                "next_steps": [
                    {"tool": "status", "args": {"operation_id": operation_id}},
                    {"tool": "await", "args": {"tools": "", "timeout_seconds": 360}}
                ]
            }
        });

        Ok(CallToolResult::success(vec![
            Content::text(result_message),
            Content::text(suggestion.to_string()),
        ]))
    }
}
