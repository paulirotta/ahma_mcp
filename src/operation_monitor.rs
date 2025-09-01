//! Operation monitoring and management system
//!
//! This module provides comprehensive monitoring, timeout handling, and cancellation
//! support for long-running cargo operations. It enables tracking of operation state,
//! automatic cleanup, and detailed logging for debugging.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
/// Information about a running operation
pub struct Operation {
    pub id: String,
    pub description: String,
    pub state: OperationStatus,
    pub result: Option<Value>,
}

impl Operation {
    /// Create a new operation info
    pub fn new(id: String, description: String, result: Option<Value>) -> Self {
        Self {
            id,
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
    #[allow(dead_code)]
    config: MonitorConfig,
}

impl OperationMonitor {
    /// Create a new operation monitor
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub async fn add_operation(&self, operation: Operation) {
        let mut ops = self.operations.write().await;
        ops.insert(operation.id.clone(), operation);
    }

    pub async fn update_status(
        &self,
        operation_id: &str,
        status: OperationStatus,
        result: Option<Value>,
    ) {
        let mut ops = self.operations.write().await;
        if let Some(op) = ops.get_mut(operation_id) {
            op.state = status;
            op.result = result;
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

        // Remove completed operations from tracking to prevent re-notification
        for op in &completed_ops {
            ops.remove(&op.id);
        }

        completed_ops
    }
}
