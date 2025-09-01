//! # MCP Client for Pushing Results
//!
//! This module provides a `Client` struct that is responsible for sending asynchronous
//! `mcp.result` notifications back to the connected language model.

use rmcp::model::{self as mcp_model, Content, ErrorData};
use rmcp::server::Control;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::operation_monitor::OperationInfo;

#[derive(Debug, Clone)]
pub struct Client {
    /// The control handle allows sending messages back to the client.
    control: Arc<Mutex<Option<Control>>>,
}

impl Client {
    /// Creates a new `Client`.
    pub fn new() -> Self {
        Self {
            control: Arc::new(Mutex::new(None)),
        }
    }

    /// Sets the control handle for the client, enabling it to send messages.
    /// This is typically called once the server has established a connection.
    pub fn set_control(&self, control: Control) {
        let mut guard = self.control.blocking_lock();
        *guard = Some(control);
    }

    /// Pushes the result of a completed operation back to the language model.
    pub async fn push_result(&self, op: &OperationInfo) {
        let mut guard = self.control.lock().await;
        if let Some(control) = guard.as_mut() {
            info!("Pushing result for completed operation: {}", op.id);

            let result_payload = match &op.result {
                Some(Ok(output)) => json!({
                    "job_id": op.id,
                    "status": "success",
                    "output": output,
                }),
                Some(Err(error)) => json!({
                    "job_id": op.id,
                    "status": "failure",
                    "error": error,
                }),
                None => json!({
                    "job_id": op.id,
                    "status": "failure",
                    "error": "Operation completed without a result.",
                }),
            };

            let mcp_result = mcp_model::Notification::new(
                "mcp.result".to_string(),
                Some(result_payload.as_object().unwrap().clone()),
            );

            if let Err(e) = control.send_notification(mcp_result).await {
                warn!("Failed to send mcp.result notification: {}", e);
            }
        } else {
            warn!("Cannot push result: MCP control handle is not set.");
        }
    }
}
