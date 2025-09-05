//! # MCP Callback Sender Implementation
//!
//! This module provides the `McpCallbackSender`, a concrete implementation of the
//! `CallbackSender` trait that integrates with the `rmcp` framework. Its purpose is to
//! translate the internal `ProgressUpdate` enum into MCP-compliant `ProgressNotificationParam`
//! messages and send them to the connected AI client.
//!
//! ## Core Components
//!
//! - **`McpCallbackSender`**: A struct that holds a reference to the `rmcp` `Peer` and the
//!   `operation_id` for the current task. The `Peer` is used to send notifications back
//!   to the client.
//!
//! ## How It Works
//!
//! 1. **Instantiation**: An `McpCallbackSender` is created with a `Peer` object (representing
//!    the connection to the client) and the unique `operation_id` for the task that will be
//!    monitored.
//!
//! 2. **`send_progress` Implementation**: This is the core of the module. The `async` method
//!    takes a `ProgressUpdate` and performs a `match` on its variant.
//!
//! 3. **Translation**: Each `ProgressUpdate` variant is translated into a corresponding
//!    `ProgressNotificationParam`. This involves mapping the internal state (like `Started`,
//!    `Progress`, `Completed`) to the fields expected by the MCP specification (like
//!    `progress`, `total`, and `message`). For example:
//!    - `ProgressUpdate::Started` sets the progress to 0%.
//!    - `ProgressUpdate::Progress` maps the percentage directly.
//!    - `ProgressUpdate::Completed` or `Failed` sets the progress to 100% to signify the
//!      end of the operation.
//!    - `ProgressUpdate::FinalResult` formats a comprehensive summary message for the client.
//!
//! 4. **Notification**: The constructed `ProgressNotificationParam` is sent to the client
//!    using `self.peer.notify_progress().await`. Any errors in sending the notification
//!    are wrapped in a `CallbackError`.
//!
//! ## Purpose in the System
//!
//! The `McpCallbackSender` acts as the bridge between the server's internal, abstract
//! progress monitoring system (`callback_system`) and the external, standardized MCP
//! communication protocol. This separation of concerns allows the core application logic
//! to remain agnostic of the specific protocol being used to communicate with the client,
//! making the system more modular and easier to maintain.

use crate::callback_system::{CallbackError, CallbackSender, ProgressUpdate};
use async_trait::async_trait;
use rmcp::{
    model::{NumberOrString, ProgressNotificationParam, ProgressToken},
    service::{Peer, RoleServer},
};
use std::sync::Arc;
use tracing;

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
        tracing::debug!("Sending MCP progress notification: {:?}", update);

        let progress_token = ProgressToken(NumberOrString::String(Arc::from(
            self.operation_id.as_str(),
        )));

        let params = match update {
            ProgressUpdate::Started {
                operation_id,
                command,
                description,
            } => {
                tracing::debug!(
                    "Starting operation {}: {} - {}",
                    operation_id,
                    command,
                    description
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
                line, is_stderr, ..
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
                tracing::debug!("Completed operation {}: {}", operation_id, message);
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
                tracing::warn!("Failed operation {}: {}", operation_id, error);
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
                tracing::warn!("Cancelled operation {}: {}", operation_id, message);

                // Enhanced cancellation message formatting to avoid "Cancelled: Canceled" repetition
                let formatted_message = if message.to_lowercase().starts_with("cancel") {
                    // Message already indicates cancellation, don't prepend "Cancelled:"
                    if message == "Canceled" {
                        "Operation cancelled by user request".to_string()
                    } else {
                        message.clone()
                    }
                } else {
                    // Message doesn't indicate cancellation, prepend "Cancelled:"
                    format!("Cancelled: {message}")
                };

                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: 100.0, // Mark as complete even when cancelled
                    total: Some(100.0),
                    message: Some(formatted_message),
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
                tracing::debug!("{} operation {}: {}", status, operation_id, command);

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
                tracing::debug!("Successfully sent MCP progress notification");
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to send MCP progress notification: {:?}", e);
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
