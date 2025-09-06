//! # MCP Client for Pushing Results and Test I/O
//!
//! This module provides:
//! 1. A `Client` struct for sending asynchronous `mcp.result` notifications.
//! 2. An `McpIo` trait for abstracting I/O.
//! 3. A `MockIo` implementation for testing.
//! 4. A `StdioMcpIo` for production.

use crate::operation_monitor::Operation;
use async_trait::async_trait;
use rmcp::model::{
    LoggingLevel, LoggingMessageNotification, LoggingMessageNotificationParam, ServerNotification,
};
use rmcp::service::{Peer, RoleServer};
use serde_json::json;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct Client {
    /// The control handle allows sending messages back to the client.
    control: Arc<Mutex<Option<Peer<RoleServer>>>>,
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
    pub fn set_control(&self, control: Peer<RoleServer>) {
        let mut guard = self.control.blocking_lock();
        *guard = Some(control);
    }

    /// Pushes the result of a completed operation back to the language model
    /// by wrapping it in a `LoggingMessageNotification`.
    pub async fn push_result(&self, op: &Operation) {
        let control_guard = self.control.lock().await;
        if let Some(control) = &*control_guard {
            let result_data = json!({
                "operationId": op.id,
                "status": op.state,
                "toolName": op.tool_name,
                "result": op.result,
            });

            let notification = ServerNotification::LoggingMessageNotification(
                LoggingMessageNotification::new(LoggingMessageNotificationParam {
                    level: LoggingLevel::Info,
                    logger: Some("ahma_mcp.operation_monitor".to_string()),
                    data: result_data,
                }),
            );

            if let Err(e) = control.send_notification(notification).await {
                warn!("Failed to send operation result notification: {}", e);
            } else {
                info!("Successfully sent result for operation {}", op.id);
            }
        } else {
            warn!(
                "Control handle not set. Could not send result for operation {}",
                op.id
            );
        }
    }
}

/// A trait for handling I/O in the MCP service.
#[async_trait]
pub trait McpIo: Send + Sync {
    async fn send(&self, message: &str);
    async fn read_line(&mut self) -> Option<String>;
}

/// A mock I/O handler for testing purposes.
#[derive(Clone)]
pub struct MockIo {
    pub input: Arc<Mutex<Receiver<String>>>,
    pub output: Sender<String>,
}

impl MockIo {
    /// Creates a new `MockIo` instance along with channels for testing.
    pub fn new() -> (Self, Sender<String>, Receiver<String>) {
        let (input_tx, input_rx) = mpsc::channel(100);
        let (output_tx, output_rx) = mpsc::channel(100);

        let mock_io = Self {
            input: Arc::new(Mutex::new(input_rx)),
            output: output_tx,
        };

        (mock_io, input_tx, output_rx)
    }
}

#[async_trait]
impl McpIo for MockIo {
    async fn send(&self, message: &str) {
        self.output.send(message.to_string()).await.unwrap();
    }

    async fn read_line(&mut self) -> Option<String> {
        self.input.lock().await.recv().await
    }
}

/// The standard I/O handler for production use.
pub struct StdioMcpIo;

#[async_trait]
impl McpIo for StdioMcpIo {
    async fn send(&self, message: &str) {
        println!("{}", message);
        io::stdout().flush().unwrap();
    }

    async fn read_line(&mut self) -> Option<String> {
        let result = tokio::task::spawn_blocking(move || {
            let mut buffer = String::new();
            if std::io::stdin().read_line(&mut buffer).unwrap_or(0) > 0 {
                Some(buffer)
            } else {
                None
            }
        })
        .await;

        result.ok().flatten()
    }
}
