//! # MCP Client for Pushing Results and Test I/O
//!
//! This module provides:
//! 1. A `Client` struct for sending asynchronous `mcp.result` notifications.
//! 2. An `McpIo` trait for abstracting I/O.
//! 3. A `MockIo` implementation for testing.
//! 4. A `StdioMcpIo` for production.

use crate::operation_monitor::Operation;
use async_trait::async_trait;
use rmcp::service::{Peer, RoleServer};
use std::sync::Arc;
use tokio::{
    io::{AsyncWriteExt, stdout},
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct Client {
    /// The control handle allows sending messages back to the client.
    control: Arc<Mutex<Option<Peer<RoleServer>>>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
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

    /// Pushes the result of a completed operation back to the language model.
    pub async fn push_result(&self, op: &Operation) {
        warn!(
            "push_result is currently a no-op. Operation {} result not sent.",
            op.id
        );
        // TODO: Re-implement this with a robust notification queuing system.
        // The previous implementation was flawed and did not align with the rmcp crate's API.
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
        stdout().flush().await.unwrap();
    }

    async fn read_line(&mut self) -> Option<String> {
        let result = tokio::task::spawn_blocking(|| {
            let mut buffer = String::new();
            if std::io::stdin().read_line(&mut buffer).unwrap_or(0) > 0 {
                Some(buffer.trim_end().to_string())
            } else {
                None
            }
        })
        .await;

        result.ok().flatten()
    }
}
