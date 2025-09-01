//! Operation monitoring and management system
//!
//! This module provides comprehensive monitoring, timeout handling, and cancellation
//! support for long-running cargo operations. It enables tracking of operation state,
//! automatic cleanup, and detailed logging for debugging.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Represents the current state of an operation
pub enum OperationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl OperationStatus {
    /// Check if this state represents a terminal (completed) operation
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OperationStatus::Completed
                | OperationStatus::Failed
                | OperationStatus::Cancelled
                | OperationStatus::TimedOut
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Information about a running operation
pub struct Operation {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub state: OperationStatus,
    pub result: Option<Value>,
}

impl Operation {
    /// Create a new operation info
    pub fn new(id: String, tool_name: String, description: String, result: Option<Value>) -> Self {
        Self {
            id,
            tool_name,
            description,
            state: OperationStatus::Pending,
            result,
        }
    }
}

/// Configuration for operation monitoring
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Default timeout for operations
    pub default_timeout: Duration,
}

impl MonitorConfig {
    /// Create a MonitorConfig with a custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            default_timeout: timeout,
        }
    }
}

/// Operation monitor that tracks and manages cargo operations
#[derive(Debug, Clone)]
pub struct OperationMonitor {
    operations: Arc<RwLock<HashMap<String, Operation>>>,
    completion_history: Arc<RwLock<HashMap<String, Operation>>>,
    #[allow(dead_code)]
    config: MonitorConfig,
}

impl OperationMonitor {
    /// Create a new operation monitor
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            completion_history: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub async fn add_operation(&self, operation: Operation) {
        let mut ops = self.operations.write().await;
        tracing::info!(
            "Adding operation to monitor: {} (status: {:?})",
            operation.id,
            operation.state
        );
        ops.insert(operation.id.clone(), operation);
        tracing::debug!("Total operations in monitor after add: {}", ops.len());
    }

    pub async fn get_operation(&self, operation_id: &str) -> Option<Operation> {
        let ops = self.operations.read().await;
        ops.get(operation_id).cloned()
    }

    pub async fn get_all_operations(&self) -> Vec<Operation> {
        let ops = self.operations.read().await;
        ops.values().cloned().collect()
    }

    pub async fn update_status(
        &self,
        operation_id: &str,
        status: OperationStatus,
        result: Option<Value>,
    ) {
        let mut ops = self.operations.write().await;
        if let Some(op) = ops.get_mut(operation_id) {
            tracing::debug!(
                "Updating operation {} from {:?} to {:?}",
                operation_id,
                op.state,
                status
            );
            op.state = status;
            op.result = result;

            if status.is_terminal() {
                let mut history = self.completion_history.write().await;
                history.insert(operation_id.to_string(), op.clone());
            }
        } else {
            tracing::warn!(
                "Attempted to update non-existent operation: {}",
                operation_id
            );
        }
    }

    pub async fn get_completed_operations(&self) -> Vec<Operation> {
        let ops = self.operations.read().await;
        ops.values()
            .filter(|op| op.state.is_terminal())
            .cloned()
            .collect()
    }

    /// Get completed operations and remove them from tracking.
    /// This prevents endless notification loops by ensuring each completion is only notified once.
    pub async fn get_and_clear_completed_operations(&self) -> Vec<Operation> {
        let mut ops = self.operations.write().await;
        let completed_ops: Vec<Operation> = ops
            .values()
            .filter(|op| op.state.is_terminal())
            .cloned()
            .collect();

        // DEBUG: Log the state before clearing
        if !completed_ops.is_empty() {
            tracing::info!(
                "CLEARING {} completed operations from monitor: {:?}",
                completed_ops.len(),
                completed_ops
                    .iter()
                    .map(|op| (&op.id, &op.state))
                    .collect::<Vec<_>>()
            );
            tracing::info!("Total operations before clearing: {}", ops.len());
        }

        // Remove completed operations from tracking to prevent re-notification
        for op in &completed_ops {
            let removed = ops.remove(&op.id);
            tracing::debug!(
                "Attempted to remove operation {}: {}",
                op.id,
                if removed.is_some() {
                    "SUCCESS"
                } else {
                    "NOT_FOUND"
                }
            );
        }

        // DEBUG: Log the state after clearing
        if !completed_ops.is_empty() {
            tracing::info!("Total operations after clearing: {}", ops.len());
        }

        completed_ops
    }

    pub async fn wait_for_operation(&self, operation_id: &str) -> Option<Operation> {
        loop {
            // Check active operations
            if let Some(op) = self.get_operation(operation_id).await {
                if op.state.is_terminal() {
                    return Some(op);
                }
            } else {
                // Check completion history
                let history = self.completion_history.read().await;
                if let Some(op) = history.get(operation_id) {
                    return Some(op.clone());
                }
                // If not in active or history, it's gone.
                return None;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}
