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
use tokio_util::sync::CancellationToken;
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
    /// Cancellation token for this operation (not serialized)
    #[serde(skip)]
    pub cancellation_token: CancellationToken,
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
            cancellation_token: CancellationToken::new(),
        }
    }
}

/// Configuration for operation monitoring
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Default timeout for operations
    pub default_timeout: Duration,
    /// Maximum time to await for graceful shutdown
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
            shutdown_timeout: Duration::from_secs(360), // Default 360 second shutdown timeout
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

    /// Cancel an operation by ID with an optional reason string.
    /// Returns true if the operation was found and cancelled, false if not found
    pub async fn cancel_operation_with_reason(
        &self,
        operation_id: &str,
        reason: Option<String>,
    ) -> bool {
        let mut ops = self.operations.write().await;
        if let Some(op) = ops.get_mut(operation_id) {
            // Only cancel if the operation is not already terminal
            if !op.state.is_terminal() {
                tracing::info!("Cancelling operation: {}", operation_id);
                tracing::debug!(
                    "CANCEL_OPERATION_WITH_REASON: operation_id='{}', reason={:?}, current_state={:?}",
                    operation_id,
                    reason,
                    op.state
                );

                op.state = OperationStatus::Cancelled;
                op.end_time = Some(std::time::SystemTime::now());
                op.cancellation_token.cancel();

                // Store structured cancellation info in result for debugging/LLM visibility
                let mut data = serde_json::Map::new();
                data.insert("cancelled".to_string(), serde_json::Value::Bool(true));
                if let Some(r) = reason.clone() {
                    tracing::debug!("Storing cancellation reason: '{}'", r);
                    data.insert("reason".to_string(), serde_json::Value::String(r));
                } else {
                    tracing::debug!("No specific cancellation reason provided, using default");
                    data.insert(
                        "reason".to_string(),
                        serde_json::Value::String("Cancelled by user".to_string()),
                    );
                }
                op.result = Some(serde_json::Value::Object(data));

                // Move to completion history
                let cancelled_op = op.clone();
                drop(ops); // Release the write lock early

                let mut history = self.completion_history.write().await;
                history.insert(operation_id.to_string(), cancelled_op);
                tracing::debug!(
                    "Moved cancelled operation {} to completion history.",
                    operation_id
                );

                // Remove from active operations
                let mut ops = self.operations.write().await;
                ops.remove(operation_id);

                true
            } else {
                tracing::warn!(
                    "Attempted to cancel already terminal operation: {}",
                    operation_id
                );
                false
            }
        } else {
            tracing::warn!(
                "Attempted to cancel non-existent operation: {}",
                operation_id
            );
            false
        }
    }

    /// Backward-compatible helper without explicit reason
    pub async fn cancel_operation(&self, operation_id: &str) -> bool {
        self.cancel_operation_with_reason(operation_id, None).await
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

        loop {
            // Check completion history first. This is the most common case for a waiting operation.
            let history = self.completion_history.read().await;
            if let Some(op) = history.get(operation_id) {
                return Some(op.clone());
            }
            drop(history);

            // Then check active operations.
            let ops = self.operations.read().await;
            if let Some(op) = ops.get(operation_id) {
                if op.state.is_terminal() {
                    // This case can happen in a race condition where the op becomes terminal
                    // but hasn't been moved to history yet.
                    return Some(op.clone());
                }
            } else {
                // If it's not in active ops, and wasn't in history, it's gone.
                return None;
            }
            drop(ops); // Release read lock

            // If we've been waiting for too long, give up.
            if start.elapsed() > timeout {
                tracing::warn!("Wait for operation {} timed out.", operation_id);
                return None;
            }

            // Wait a bit before polling again
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    }

    /// Advanced await functionality that waits for multiple operations with progressive timeout warnings.
    /// This method implements the enhanced await functionality with timeout validation, tool filtering,
    /// and progressive user feedback.
    ///
    /// # Arguments
    /// * `tool_filter` - Optional comma-separated list of tool prefixes to await for
    /// * `timeout_seconds` - Timeout in seconds (1-1800 range, defaults to 240)
    ///
    /// # Returns
    /// A vector of completed operations that match the filter criteria
    pub async fn wait_for_operations_advanced(
        &self,
        tool_filter: Option<&str>,
        timeout_seconds: Option<u32>,
    ) -> Vec<Operation> {
        // Validate and set timeout (1-1800 seconds, default 240)
        let timeout_secs = timeout_seconds.unwrap_or(240).clamp(1, 1800);
        let timeout = Duration::from_secs(timeout_secs as u64);
        let start_time = std::time::Instant::now();

        // Parse tool filter
        let tool_filters: Option<Vec<String>> = tool_filter.map(|filters| {
            filters
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .collect()
        });

        tracing::info!(
            "Starting advanced await operation: timeout={}s, tool_filter={:?}",
            timeout_secs,
            tool_filters
        );

        // Progressive warning thresholds (50%, 75%, 90%)
        const WARNING_THRESHOLDS: [u8; 3] = [50, 75, 90];
        let mut warnings_sent = [false; 3];

        let mut completed_operations = Vec::new();

        loop {
            let elapsed = start_time.elapsed();
            let progress_percent = (elapsed.as_secs_f64() / timeout.as_secs_f64() * 100.0) as u8;

            // Check for timeout
            if elapsed >= timeout {
                tracing::warn!("Advanced await operation timed out after {}s", timeout_secs);
                break;
            }

            // Send progressive warnings
            for (i, &threshold) in WARNING_THRESHOLDS.iter().enumerate() {
                if progress_percent >= threshold && !warnings_sent[i] {
                    warnings_sent[i] = true;
                    let remaining_secs = timeout_secs as i64 - elapsed.as_secs() as i64;

                    match threshold {
                        50 => tracing::warn!(
                            "⏰ Wait operation 50% complete - {}s remaining. Current active operations being monitored.",
                            remaining_secs.max(0)
                        ),
                        75 => tracing::warn!(
                            "⚠️ Wait operation 75% complete - {}s remaining. Consider checking operation status.",
                            remaining_secs.max(0)
                        ),
                        90 => tracing::warn!(
                            "🚨 Wait operation 90% complete - {}s remaining. Operations may timeout soon!",
                            remaining_secs.max(0)
                        ),
                        _ => {}
                    }
                }
            }

            // Get currently active operations that match our filter
            let active_ops = {
                let ops = self.operations.read().await;
                ops.values()
                    .filter(|op| !op.state.is_terminal())
                    .filter(|op| {
                        if let Some(ref filters) = tool_filters {
                            filters
                                .iter()
                                .any(|filter| op.tool_name.to_lowercase().starts_with(filter))
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            };

            // If no active operations match our criteria, collect completed ones and exit
            if active_ops.is_empty() {
                let history = self.completion_history.read().await;
                completed_operations = history
                    .values()
                    .filter(|op| {
                        if let Some(ref filters) = tool_filters {
                            filters
                                .iter()
                                .any(|filter| op.tool_name.to_lowercase().starts_with(filter))
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect();

                tracing::info!(
                    "Advanced await completed: {} operations finished, no active operations remaining",
                    completed_operations.len()
                );
                break;
            }

            // Check if any operations completed and moved to history
            let history = self.completion_history.read().await;
            let newly_completed: Vec<_> = history
                .values()
                .filter(|op| {
                    if let Some(ref filters) = tool_filters {
                        filters
                            .iter()
                            .any(|filter| op.tool_name.to_lowercase().starts_with(filter))
                    } else {
                        true
                    }
                })
                .filter(|op| {
                    // Only include operations completed since we started waiting
                    if let Some(end_time) = op.end_time {
                        let wait_start_system_time = std::time::SystemTime::now() - elapsed;
                        end_time >= wait_start_system_time
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();

            completed_operations.extend(newly_completed);
            drop(history);

            // Sleep before next check
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        completed_operations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// This test simulates the race condition where an operation completes
    /// so quickly that a `await` call might miss it. By using the `completion_history`
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

        // 3. Now, try to await for it. The old system might have failed here.
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
