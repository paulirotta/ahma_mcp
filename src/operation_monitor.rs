//! Operation monitoring and management system
//!
//! This module provides comprehensive monitoring, timeout handling, and cancellation
//! support for long-running operations. It enables tracking of operation state,
//! automatic cleanup, and detailed logging for debugging.

use crate::callback_system::{CallbackSender, ProgressUpdate};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use uuid::Uuid;

/// Represents the current state of an operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl Default for OperationState {
    fn default() -> Self {
        Self::Pending
    }
}

impl OperationState {
    /// Check if this state represents an active (non-terminal) operation
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

/// Information about a running operation
#[derive(Debug, Clone)]
pub struct OperationInfo {
    pub id: String,
    pub command: String,
    pub description: String,
    pub state: OperationState,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub timeout_duration: Option<Duration>,
    pub working_directory: Option<String>,
    pub result: Option<Result<String, String>>,
    pub cancellation_token: CancellationToken,
}

impl OperationInfo {
    /// Create a new operation info
    pub fn new(
        command: String,
        description: String,
        timeout_duration: Option<Duration>,
        working_directory: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            command,
            description,
            state: OperationState::Pending,
            start_time: Instant::now(),
            end_time: None,
            timeout_duration,
            working_directory,
            result: None,
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Get the duration since the operation started
    pub fn duration(&self) -> Duration {
        match self.end_time {
            Some(end) => end.duration_since(self.start_time),
            None => self.start_time.elapsed(),
        }
    }

    /// Check if the operation is still active (running or pending)
    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Mark the operation as completed with a result
    pub fn complete(&mut self, result: Result<String, String>) {
        self.end_time = Some(Instant::now());
        self.state = match &result {
            Ok(_) => OperationState::Completed,
            Err(_) => OperationState::Failed,
        };
        self.result = Some(result);
    }

    /// Mark the operation as cancelled
    pub fn cancel(&mut self) {
        self.end_time = Some(Instant::now());
        self.state = OperationState::Cancelled;
        self.cancellation_token.cancel();
    }

    /// Mark the operation as timed out
    pub fn timeout(&mut self) {
        self.end_time = Some(Instant::now());
        self.state = OperationState::TimedOut;
        self.cancellation_token.cancel();
    }

    /// Start the operation (change state from Pending to Running)
    pub fn start(&mut self) {
        if self.state == OperationState::Pending {
            self.state = OperationState::Running;
        }
    }
}

/// Configuration for operation monitoring
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub default_timeout: Duration,
    pub cleanup_interval: Duration,
    pub max_history_size: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(300),  // 5 minutes
            cleanup_interval: Duration::from_secs(600), // 10 minutes
            max_history_size: 1000,
        }
    }
}

/// Operation monitor that tracks and manages operations
#[derive(Debug)]
pub struct OperationMonitor {
    operations: Arc<RwLock<HashMap<String, OperationInfo>>>,
    config: MonitorConfig,
    cleanup_token: CancellationToken,
}

impl OperationMonitor {
    /// Create a new operation monitor
    pub fn new(config: MonitorConfig) -> Self {
        let monitor = Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            config,
            cleanup_token: CancellationToken::new(),
        };
        monitor.start_cleanup_task();
        monitor
    }

    /// Register a new operation for monitoring
    pub async fn register_operation(
        &self,
        command: String,
        description: String,
        timeout_duration: Option<Duration>,
        working_directory: Option<String>,
    ) -> String {
        let operation = OperationInfo::new(
            command,
            description.clone(),
            timeout_duration.or(Some(self.config.default_timeout)),
            working_directory,
        );
        let id = operation.id.clone();
        let mut operations = self.operations.write().await;
        operations.insert(id.clone(), operation);
        tracing::debug!("Registered operation {id}: {description}");
        id
    }

    /// Start monitoring an operation
    pub async fn start_operation(&self, operation_id: &str) -> Result<(), String> {
        let mut operations = self.operations.write().await;
        if let Some(operation) = operations.get_mut(operation_id) {
            operation.start();
            tracing::debug!(
                "Started operation {operation_id}: {}",
                operation.description
            );
            Ok(())
        } else {
            Err(format!("Operation not found: {operation_id}"))
        }
    }

    /// Complete an operation with a result
    pub async fn complete_operation(
        &self,
        operation_id: &str,
        result: Result<String, String>,
    ) -> Result<(), String> {
        let mut operations = self.operations.write().await;
        if let Some(operation) = operations.get_mut(operation_id) {
            operation.complete(result);
            Ok(())
        } else {
            Err(format!("Operation not found: {operation_id}"))
        }
    }

    /// Get information about an operation
    pub async fn get_operation(&self, operation_id: &str) -> Option<OperationInfo> {
        self.operations.read().await.get(operation_id).cloned()
    }

    /// Execute an operation with monitoring, timeout, and cancellation support
    pub async fn execute_with_monitoring<F, Fut>(
        &self,
        command: String,
        description: String,
        timeout_duration: Option<Duration>,
        working_directory: Option<String>,
        callback: Option<Box<dyn CallbackSender>>,
        operation: F,
    ) -> Result<String, String>
    where
        F: FnOnce(String, CancellationToken) -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        let operation_id = self
            .register_operation(
                command.clone(),
                description.clone(),
                timeout_duration,
                working_directory,
            )
            .await;

        let cancellation_token = {
            let ops = self.operations.read().await;
            ops.get(&operation_id)
                .map(|op| op.cancellation_token.clone())
                .ok_or_else(|| "Operation registration failed".to_string())?
        };

        self.start_operation(&operation_id).await?;

        if let Some(ref cb) = callback {
            let _ = cb
                .send_progress(ProgressUpdate::Started {
                    operation_id: operation_id.clone(),
                    command,
                    description,
                })
                .await;
        }

        let timeout_duration = timeout_duration.unwrap_or(self.config.default_timeout);
        let result = timeout(
            timeout_duration,
            operation(operation_id.clone(), cancellation_token),
        )
        .await;

        let final_result = match result {
            Ok(op_result) => op_result,
            Err(_) => {
                let mut ops = self.operations.write().await;
                if let Some(op) = ops.get_mut(&operation_id) {
                    op.timeout();
                }
                Err("Operation timed out".to_string())
            }
        };

        self.complete_operation(&operation_id, final_result.clone())
            .await?;

        if let Some(ref cb) = callback {
            let duration = self
                .get_operation(&operation_id)
                .await
                .map(|op| op.duration().as_millis() as u64)
                .unwrap_or(0);

            let update = match &final_result {
                Ok(msg) => ProgressUpdate::Completed {
                    operation_id: operation_id.clone(),
                    message: msg.clone(),
                    duration_ms: duration,
                },
                Err(err) => ProgressUpdate::Failed {
                    operation_id: operation_id.clone(),
                    error: err.clone(),
                    duration_ms: duration,
                },
            };
            let _ = cb.send_progress(update).await;
        }

        final_result
    }

    /// Start the cleanup task for removing old operations
    fn start_cleanup_task(&self) {
        if tokio::runtime::Handle::try_current().is_err() {
            debug!("No Tokio runtime, skipping cleanup task.");
            return;
        }
        let operations = Arc::clone(&self.operations);
        let config = self.config.clone();
        let token = self.cleanup_token.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.cleanup_interval);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::cleanup_operations(&operations, &config).await;
                    }
                    _ = token.cancelled() => {
                        info!("Cleanup task cancelled.");
                        break;
                    }
                }
            }
        });
    }

    /// Clean up old completed operations
    async fn cleanup_operations(
        operations: &Arc<RwLock<HashMap<String, OperationInfo>>>,
        config: &MonitorConfig,
    ) {
        let mut ops = operations.write().await;
        if ops.len() <= config.max_history_size {
            return;
        }

        let mut completed_ops: Vec<_> = ops
            .iter()
            .filter(|(_, op)| !op.is_active())
            .map(|(id, op)| (id.clone(), op.end_time.unwrap_or_else(Instant::now)))
            .collect();

        completed_ops.sort_by_key(|k| k.1);

        let to_remove = completed_ops.len().saturating_sub(config.max_history_size);
        for (id, _) in completed_ops.iter().take(to_remove) {
            ops.remove(id);
        }
    }
}

impl Drop for OperationMonitor {
    fn drop(&mut self) {
        self.cleanup_token.cancel();
    }
}
