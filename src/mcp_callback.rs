//! MCP Callback Sender Implementation
//!
//! This module provides McpCallbackSender, which integrates the callback system
//! with the rmcp framework to send progress notifications back to the AI client.

use crate::callback_system::{CallbackError, CallbackSender, ProgressUpdate};
use async_trait::async_trait;
use rmcp::{
    model::{NumberOrString, ProgressNotificationParam, ProgressToken},
    service::{Peer, RoleServer},
};
use std::sync::Arc;
use tracing::{debug, error, warn};

/// MCP callback sender that sends progress notifications to the AI client
pub struct McpCallbackSender {
    peer: Peer<RoleServer>,
    operation_id: String,
}

impl McpCallbackSender {
    pub fn new(peer: Peer<RoleServer>, operation_id: String) -> Self {
        Self { peer, operation_id }
    }
}

#[async_trait]
impl CallbackSender for McpCallbackSender {
    async fn send_progress(&self, update: ProgressUpdate) -> Result<(), CallbackError> {
        debug!("Sending MCP progress notification: {:?}", update);

        let progress_token = ProgressToken(NumberOrString::String(Arc::from(
            self.operation_id.as_str(),
        )));

        let params = match update {
            ProgressUpdate::Started {
                operation_id,
                command,
                description,
            } => {
                debug!(
                    "Starting operation {}: {} - {}",
                    operation_id, command, description
                );
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 0.0,
                    total: None,
                    message: Some(format!("{command}: {description}")),
                }
            }
            ProgressUpdate::Progress {
                message,
                percentage,
                ..
            } => {
                let progress = percentage.unwrap_or(50.0); // Default progress if unknown
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress,
                    total: Some(100.0),
                    message: Some(message.clone()),
                }
            }
            ProgressUpdate::Output {
                line,
                is_stderr,
                ..
            } => {
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 75.0, // Arbitrary progress for output
                    total: Some(100.0),
                    message: Some(if is_stderr {
                        format!("stderr: {line}")
                    } else {
                        format!("stdout: {line}")
                    }),
                }
            }
            ProgressUpdate::Completed {
                operation_id,
                message,
                ..
            } => {
                debug!("Completed operation {}: {}", operation_id, message);
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 100.0,
                    total: Some(100.0),
                    message: Some(message.clone()),
                }
            }
            ProgressUpdate::Failed {
                operation_id,
                error,
                ..
            } => {
                warn!("Failed operation {}: {}", operation_id, error);
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 100.0, // Mark as complete even on failure
                    total: Some(100.0),
                    message: Some(format!("Failed: {error}")),
                }
            }
            ProgressUpdate::Cancelled {
                operation_id,
                message,
                ..
            } => {
                warn!("Cancelled operation {}: {}", operation_id, message);
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 100.0, // Mark as complete even when cancelled
                    total: Some(100.0),
                    message: Some(format!("Cancelled: {message}")),
                }
            }
            ProgressUpdate::FinalResult {
                operation_id,
                command,
                description,
                working_directory,
                success,
                full_output,
                duration_ms,
            } => {
                let status = if success { "COMPLETED" } else { "FAILED" };
                debug!("{} operation {}: {}", status, operation_id, command);

                let final_message = format!(
                    "OPERATION {}: '{}'\nCommand: {}\nDescription: {}\nWorking Directory: {}\nDuration: {}ms\n\n=== FULL OUTPUT ===\n{}",
                    status,
                    operation_id,
                    command,
                    description,
                    working_directory,
                    duration_ms,
                    full_output
                );

                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 100.0,
                    total: Some(100.0),
                    message: Some(final_message),
                }
            }
        };

        match self.peer.notify_progress(params).await {
            Ok(()) => {
                debug!("Successfully sent MCP progress notification");
                Ok(())
            }
            Err(e) => {
                error!("Failed to send MCP progress notification: {:?}", e);
                Err(CallbackError::SendFailed(format!(
                    "Failed to send MCP notification: {e:?}"
                )))
            }
        }
    }

    async fn should_cancel(&self) -> bool {
        // For now, we don't support cancellation checks via MCP.
        // This could be enhanced in the future with cancellation token support.
        false
    }
}

/// Utility function to create an MCP callback sender.
pub fn mcp_callback(peer: Peer<RoleServer>, operation_id: String) -> Box<dyn CallbackSender> {
    Box::new(McpCallbackSender::new(peer, operation_id))
}
