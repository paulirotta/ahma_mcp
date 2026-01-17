//! Operation monitoring and management system
//!
//! This module provides comprehensive monitoring, timeout handling, and cancellation
//! support for long-running cargo operations. It enables tracking of operation state,
//! automatic cleanup, and detailed logging for debugging.

use crate::utils::time;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::{Notify, RwLock};
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
    #[serde(with = "time")]
    pub start_time: SystemTime,
    /// When the operation completed (None if still running)
    #[serde(with = "time::option", default)]
    pub end_time: Option<SystemTime>,
    /// When wait_for_operation was first called for this operation (None if never waited for)
    #[serde(with = "time::option", default)]
    pub first_wait_time: Option<SystemTime>,
    /// Timeout duration for this specific operation (None means use default)
    pub timeout_duration: Option<Duration>,
    /// Cancellation token for this operation (not serialized)
    #[serde(skip)]
    pub cancellation_token: CancellationToken,
    /// Notifier for when the operation completes (not serialized)
    #[serde(skip)]
    pub completion_notifier: Arc<Notify>,
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
            start_time: SystemTime::now(),
            end_time: None,
            first_wait_time: None,
            timeout_duration: None,
            cancellation_token: CancellationToken::new(),
            completion_notifier: Arc::new(Notify::new()),
        }
    }

    /// Create a new operation info with timeout
    pub fn new_with_timeout(
        id: String,
        tool_name: String,
        description: String,
        result: Option<Value>,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            id,
            tool_name,
            description,
            state: OperationStatus::Pending,
            result,
            start_time: SystemTime::now(),
            end_time: None,
            first_wait_time: None,
            timeout_duration: timeout,
            cancellation_token: CancellationToken::new(),
            completion_notifier: Arc::new(Notify::new()),
        }
    }
}

/// Configuration for operation monitoring
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Default timeout for operations (reduced from 5 minutes to 30 seconds)
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
            shutdown_timeout: Duration::from_secs(30), // Reduced from 360s to 30s
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
        // Simplified: No polling detection - just return the operation
        let ops = self.operations.read().await;
        ops.get(operation_id).cloned()
    }

    /// Starts a background task that periodically checks for timed-out operations.
    /// This ensures that operations that exceed their timeout are cancelled even if
    /// the executor fails to handle them.
    pub fn start_background_monitor(monitor: Arc<Self>) {
        tokio::spawn(async move {
            let check_interval = Duration::from_secs(1);
            loop {
                monitor.check_timeouts().await;
                tokio::time::sleep(check_interval).await;
            }
        });
    }

    /// Checks all active operations for timeouts and cancels them if necessary.
    pub async fn check_timeouts(&self) {
        let now = SystemTime::now();
        let mut timed_out_ops = Vec::new();

        {
            let ops = self.operations.read().await;
            for op in ops.values() {
                if op.state.is_terminal() {
                    continue;
                }

                let timeout = op.timeout_duration.unwrap_or(self.config.default_timeout);
                if let Ok(elapsed) = now.duration_since(op.start_time)
                    && elapsed > timeout
                {
                    timed_out_ops.push((op.id.clone(), elapsed, timeout));
                }
            }
        }

        for (op_id, elapsed, timeout) in timed_out_ops {
            let reason = format!(
                "Operation timed out after {:.1}s (limit: {:.1}s)",
                elapsed.as_secs_f64(),
                timeout.as_secs_f64()
            );
            self.timeout_operation(&op_id, reason).await;
        }
    }

    async fn timeout_operation(&self, operation_id: &str, reason: String) {
        let mut ops = self.operations.write().await;
        if let Some(op) = ops.get_mut(operation_id)
            && !op.state.is_terminal()
        {
            tracing::warn!("Timing out operation: {} - {}", operation_id, reason);

            op.state = OperationStatus::TimedOut;
            op.end_time = Some(SystemTime::now());
            op.cancellation_token.cancel();

            op.result = Some(serde_json::json!({
                "timed_out": true,
                "reason": reason
            }));

            // Remove from active operations
            let timed_out_op = ops.remove(operation_id);
            drop(ops); // Release lock

            // Move to history BEFORE notifying to avoid race condition
            if let Some(op) = timed_out_op {
                let mut history = self.completion_history.write().await;
                history.insert(operation_id.to_string(), op.clone());
                drop(history);

                // NOW notify waiters
                op.completion_notifier.notify_waiters();
            }
        }
    }

    /// Returns all currently active (non-terminal) operations.
    ///
    /// Note: Completed operations are accessible via `get_completed_operations`.
    pub async fn get_all_active_operations(&self) -> Vec<Operation> {
        let ops = self.operations.read().await;
        ops.values()
            .filter(|op| !op.state.is_terminal())
            .cloned()
            .collect()
    }

    pub async fn update_status(
        &self,
        operation_id: &str,
        status: OperationStatus,
        result: Option<Value>,
    ) {
        let mut ops = self.operations.write().await;
        let mut operation_to_move = None;

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
                op.end_time = Some(SystemTime::now());

                // Remove from active operations now (before notifying)
                operation_to_move = ops.remove(operation_id);
            }
        }

        // Drop operations lock before acquiring history lock
        drop(ops);

        // If we removed a terminal operation, move it to completion history
        // BEFORE notifying waiters to avoid race condition
        if let Some(op) = operation_to_move {
            let mut history = self.completion_history.write().await;
            history.insert(operation_id.to_string(), op.clone());
            tracing::debug!("Moved operation {} to completion history.", operation_id);
            drop(history);

            // NOW notify anyone waiting on this operation
            // The operation is guaranteed to be in completion_history at this point
            op.completion_notifier.notify_waiters();
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
                op.end_time = Some(SystemTime::now());
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
        let operations: Vec<Operation> = ops
            .values()
            .filter(|op| !op.state.is_terminal())
            .cloned()
            .collect();
        let total_active = operations.len();

        ShutdownSummary {
            total_active,
            operations,
        }
    }

    pub async fn wait_for_operation(&self, operation_id: &str) -> Option<Operation> {
        let timeout = Duration::from_secs(30); // Reduced from 300s to 30s

        // First, check if the operation has already completed.
        {
            let history = self.completion_history.read().await;
            if let Some(op) = history.get(operation_id) {
                return Some(op.clone());
            }
        } // Drop history lock immediately

        // Check if the operation exists in active operations and is already terminal
        // This handles the case where an operation is added in a terminal state
        {
            let mut ops = self.operations.write().await;
            if let Some(op) = ops.get_mut(operation_id)
                && op.state.is_terminal()
            {
                // Operation is already terminal, but we still need to set first_wait_time
                if op.first_wait_time.is_none() {
                    op.first_wait_time = Some(SystemTime::now());
                }
                return Some(op.clone());
            }
        } // Drop write lock immediately

        // Get the notifier and check/set first_wait_time atomically
        let notifier = {
            let mut ops = self.operations.write().await;
            if let Some(op) = ops.get_mut(operation_id) {
                // Double-check if it became terminal while we were waiting for the write lock
                if op.state.is_terminal() {
                    // Set first_wait_time if it hasn't been set yet
                    if op.first_wait_time.is_none() {
                        op.first_wait_time = Some(SystemTime::now());
                    }
                    return Some(op.clone());
                }

                // Set first_wait_time if it hasn't been set yet (atomic operation)
                if op.first_wait_time.is_none() {
                    op.first_wait_time = Some(SystemTime::now());
                }
                Some(op.completion_notifier.clone())
            } else {
                // Operation doesn't exist in active operations.
                None
            }
        }; // Drop write lock immediately

        if let Some(notifier) = notifier {
            // Wait for the notification or timeout.
            match tokio::time::timeout(timeout, notifier.notified()).await {
                Ok(_) => {
                    // Notification received, the operation should be in the completion history now.
                    // There's a small race window where the notifier fires before the operation
                    // is moved to completion_history. Retry a few times with a small delay.
                    for attempt in 0..10 {
                        let history = self.completion_history.read().await;
                        if let Some(op) = history.get(operation_id) {
                            return Some(op.clone());
                        }
                        drop(history);

                        if attempt < 9 {
                            tracing::debug!(
                                "Operation {} not yet in completion history, retrying (attempt {}/10)",
                                operation_id,
                                attempt + 1
                            );
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }

                    tracing::warn!(
                        "Operation {} completed but not found in history after 10 retries",
                        operation_id
                    );
                    None
                }
                Err(_) => {
                    // Timeout elapsed.
                    tracing::warn!("Wait for operation {} timed out.", operation_id);
                    None
                }
            }
        } else {
            // Operation doesn't exist
            None
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
        let start_time = Instant::now();

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

            // If there are active operations but none match the filter, avoid scanning history repeatedly
            if let Some(ref filters) = tool_filters {
                let any_match = active_ops.iter().any(|op| {
                    let name = op.tool_name.to_lowercase();
                    filters.iter().any(|f| name.starts_with(f))
                });
                if !any_match {
                    // No matching active operations; short sleep and continue
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
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
                        let wait_start_system_time = SystemTime::now() - elapsed;
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
            tokio::time::sleep(Duration::from_millis(
                crate::constants::SEQUENCE_STEP_DELAY_MS,
            ))
            .await;
        }

        completed_operations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::logging::init_test_logging;
    use std::time::Duration;

    /// This test simulates the race condition where an operation completes
    /// so quickly that a `await` call might miss it. By using the `completion_history`
    /// map, the monitor should now correctly retrieve the status of already-completed
    /// operations.
    #[tokio::test]
    async fn test_wait_for_fast_completion_race_condition() {
        init_test_logging();
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
        init_test_logging();
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
