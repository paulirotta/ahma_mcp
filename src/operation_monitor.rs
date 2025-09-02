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
    /// When the operation was created
    pub start_time: std::time::SystemTime,
    /// When the operation completed (None if still running)
    pub end_time: Option<std::time::SystemTime>,
    /// When wait_for_operation was first called for this operation (None if never waited for)
    pub first_wait_time: Option<std::time::SystemTime>,
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
            start_time: std::time::SystemTime::now(),
            end_time: None,
            first_wait_time: None,
        }
    }
}

/// Configuration for operation monitoring
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Default timeout for operations
    pub default_timeout: Duration,
    /// Maximum time to wait for graceful shutdown
    pub shutdown_timeout: Duration,
}

#[derive(Debug, Clone)]
/// Summary of active operations for shutdown coordination
pub struct ShutdownSummary {
    pub total_active: usize,
    pub operations: Vec<Operation>,
}

impl MonitorConfig {
    /// Create a MonitorConfig with a custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            default_timeout: timeout,
            shutdown_timeout: Duration::from_secs(15), // Default 15 second shutdown timeout
        }
    }

    /// Create a MonitorConfig with custom timeouts
    pub fn with_timeouts(operation_timeout: Duration, shutdown_timeout: Duration) -> Self {
        Self {
            default_timeout: operation_timeout,
            shutdown_timeout,
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

            // Set end time when operation reaches terminal state
            if status.is_terminal() {
                op.end_time = Some(std::time::SystemTime::now());
            }
        }

        // If the operation has reached a terminal state, move it from the active map
        // to the completion history. This is the key to preventing race conditions.
        if status.is_terminal() {
            if let Some(op) = ops.remove(operation_id) {
                let mut history = self.completion_history.write().await;
                history.insert(operation_id.to_string(), op);
                tracing::debug!("Moved operation {} to completion history.", operation_id);
            }
        }
    }

    pub async fn get_active_operations(&self) -> Vec<Operation> {
        let ops = self.operations.read().await;
        ops.values()
            .filter(|op| !op.state.is_terminal())
            .cloned()
            .collect()
    }

    pub async fn get_completed_operations(&self) -> Vec<Operation> {
        let history = self.completion_history.read().await;
        history.values().cloned().collect()
    }

    pub async fn get_shutdown_summary(&self) -> ShutdownSummary {
        let ops = self.operations.read().await;
        let active_ops: Vec<&Operation> =
            ops.values().filter(|op| !op.state.is_terminal()).collect();

        ShutdownSummary {
            total_active: active_ops.len(),
            operations: active_ops.into_iter().cloned().collect(),
        }
    }

    pub async fn wait_for_operation(&self, operation_id: &str) -> Option<Operation> {
        let timeout = Duration::from_secs(300); // 5 minute timeout to prevent indefinite waiting
        let start = std::time::Instant::now();

        // Record that someone is waiting for this operation (for metrics)
        {
            let mut ops = self.operations.write().await;
            if let Some(op) = ops.get_mut(operation_id) {
                if op.first_wait_time.is_none() {
                    op.first_wait_time = Some(std::time::SystemTime::now());
                }
            }
        }

        // First, do an initial check to see if operation exists at all
        let exists_in_active = {
            let ops = self.operations.read().await;
            ops.contains_key(operation_id)
        };

        let exists_in_history = {
            let history = self.completion_history.read().await;
            history.contains_key(operation_id)
        };

        // If operation doesn't exist anywhere, return None immediately
        if !exists_in_active && !exists_in_history {
            tracing::warn!(
                "Operation {} not found in active operations or completion history",
                operation_id
            );
            return None;
        }

        loop {
            // Check active operations first
            let ops = self.operations.read().await;
            if let Some(op) = ops.get(operation_id) {
                if op.state.is_terminal() {
                    // This case can happen in a race condition where the op becomes terminal
                    // but hasn't been moved to history yet.
                    return Some(op.clone());
                }
            }
            drop(ops); // Release read lock

            // If not in active, check completion history. This is the primary path for completed ops.
            let history = self.completion_history.read().await;
            if let Some(op) = history.get(operation_id) {
                return Some(op.clone());
            }
            drop(history);

            // If we've been waiting for too long, give up.
            if start.elapsed() > timeout {
                tracing::warn!("Wait for operation {} timed out.", operation_id);
                return None;
            }

            // Wait a bit before polling again
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// This test simulates the race condition where an operation completes
    /// so quickly that a `wait` call might miss it. By using the `completion_history`
    /// map, the monitor should now correctly retrieve the status of already-completed
    /// operations.
    #[tokio::test]
    async fn test_wait_for_fast_completion_race_condition() {
        let monitor = OperationMonitor::new(MonitorConfig::with_timeout(Duration::from_secs(5)));
        let op_id = "fast_op_1".to_string();
        let op = Operation::new(
            op_id.clone(),
            "test_tool".to_string(),
            "A test operation".to_string(),
            None,
        );

        // 1. Add the operation
        monitor.add_operation(op).await;

        // 2. Immediately update its status to Completed, which moves it to history
        monitor
            .update_status(
                &op_id,
                OperationStatus::Completed,
                Some(serde_json::json!({"result": "success"})),
            )
            .await;

        // 3. Now, try to wait for it. The old system might have failed here.
        let result = monitor.wait_for_operation(&op_id).await;

        // 4. Assert that we correctly found the completed operation.
        assert!(result.is_some());
        let completed_op = result.unwrap();
        assert_eq!(completed_op.id, op_id);
        assert_eq!(completed_op.state, OperationStatus::Completed);
        assert_eq!(
            completed_op.result,
            Some(serde_json::json!({"result": "success"}))
        );

        // 5. Verify it's not in the active operations map anymore
        let active_ops = monitor.operations.read().await;
        assert!(!active_ops.contains_key(&op_id));

        // 6. Verify it IS in the completion history map
        let history = monitor.completion_history.read().await;
        assert!(history.contains_key(&op_id));
    }

    /// Tests that waiting for an operation that never existed returns `None`
    /// immediately instead of blocking indefinitely.
    #[tokio::test]
    async fn test_wait_for_nonexistent_operation() {
        let monitor = OperationMonitor::new(MonitorConfig::with_timeout(Duration::from_secs(5)));

        // Use a timeout to ensure the test completes quickly
        let wait_result = tokio::time::timeout(
            Duration::from_millis(200),
            monitor.wait_for_operation("nonexistent-id"),
        )
        .await;

        // Should complete quickly and return None for nonexistent operation
        match wait_result {
            Ok(result) => {
                assert!(
                    result.is_none(),
                    "Should return None for nonexistent operation"
                );
            }
            Err(_) => {
                panic!(
                    "wait_for_operation should return quickly for nonexistent operation, not timeout"
                );
            }
        }
    }
}
