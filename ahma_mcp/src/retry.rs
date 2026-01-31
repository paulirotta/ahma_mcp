//! Retry Logic with Exponential Backoff
//!
//! This module provides retry functionality for transient failures with configurable
//! exponential backoff. It intelligently distinguishes between retryable (transient)
//! and non-retryable (permanent) errors.
//!
//! ## Key Features
//!
//! - **Exponential Backoff**: Delays between retries grow exponentially (100ms, 200ms, 400ms...)
//! - **Max Delay Cap**: Prevents unbounded delay growth
//! - **Jitter**: Optional randomization to prevent thundering herd
//! - **Smart Error Classification**: Only retries transient errors
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ahma_mcp::retry::{RetryConfig, execute_with_retry};
//!
//! let config = RetryConfig::default();
//! let result = execute_with_retry(&config, || async {
//!     // Your fallible operation here
//!     Ok::<_, anyhow::Error>("success".to_string())
//! }).await;
//! ```

use rand::Rng;
use std::future::Future;
use std::time::Duration;

/// Retry policy strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RetryPolicy {
    /// Delays grow exponentially: initial_delay * backoff_factor^attempt
    #[default]
    ExponentialBackoff,
    /// Fixed delay between all retries
    FixedDelay,
    /// No delay between retries
    Immediate,
}

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not counting initial try)
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay cap (prevents unbounded growth)
    pub max_delay: Duration,
    /// Multiplier for each subsequent retry delay
    pub backoff_factor: f64,
    /// The retry policy to use
    pub policy: RetryPolicy,
    /// Whether to add jitter to delays
    pub jitter_enabled: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_factor: 2.0,
            policy: RetryPolicy::ExponentialBackoff,
            jitter_enabled: false,
        }
    }
}

impl RetryConfig {
    /// Create a new RetryConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the initial delay before first retry
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set the maximum delay cap
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the backoff factor (multiplier for each retry)
    pub fn with_backoff_factor(mut self, factor: f64) -> Self {
        self.backoff_factor = factor;
        self
    }

    /// Set the retry policy
    pub fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Enable or disable jitter
    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.jitter_enabled = enabled;
        self
    }

    /// Calculate the delay for a given retry attempt (0-indexed)
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = match self.policy {
            RetryPolicy::ExponentialBackoff => {
                let multiplier = self.backoff_factor.powi(attempt as i32);
                let delay_ms = self.initial_delay.as_millis() as f64 * multiplier;
                Duration::from_millis(delay_ms as u64)
            }
            RetryPolicy::FixedDelay => self.initial_delay,
            RetryPolicy::Immediate => Duration::ZERO,
        };

        // Cap at max_delay
        std::cmp::min(delay, self.max_delay)
    }

    /// Calculate delay with optional jitter (±50% of base delay)
    pub fn delay_for_attempt_with_jitter(&self, attempt: u32) -> Duration {
        let base_delay = self.delay_for_attempt(attempt);

        if !self.jitter_enabled {
            return base_delay;
        }

        let base_ms = base_delay.as_millis() as f64;
        // Add jitter: ±50% of base delay
        let jitter_range = base_ms * 0.5;
        let jitter = rand::rng().random_range(-jitter_range..jitter_range);
        let jittered_ms = (base_ms + jitter).max(0.0);

        Duration::from_millis(jittered_ms as u64)
    }
}

/// Patterns indicating transient (retryable) errors
const RETRYABLE_PATTERNS: &[&str] = &[
    // Timeout-related
    "timed out",
    "timeout",
    "etimedout",
    // Resource exhaustion
    "resource temporarily unavailable",
    "eagain",
    "resource busy",
    "too many open files",
    // Network issues
    "connection reset",
    "broken pipe",
    "network is unreachable",
    "connection refused",
    "no route to host",
    // Process issues
    "process not responding",
    "deadlock",
    // I/O retry scenarios
    "interrupted system call",
    "eintr",
];

/// Patterns indicating permanent (non-retryable) errors
const PERMANENT_PATTERNS: &[&str] = &[
    // Permission issues
    "permission denied",
    "access denied",
    "operation not permitted",
    // File/path issues
    "no such file or directory",
    "file not found",
    "not a directory",
    "is a directory",
    // Command issues
    "command not found",
    "not found",
    "syntax error",
    "invalid argument",
    "invalid option",
    // Authentication
    "authentication failed",
    "unauthorized",
];

/// Check if an error message indicates a retryable (transient) error
pub fn is_retryable_error(error_message: &str) -> bool {
    let lower = error_message.to_lowercase();

    // First check if it matches any permanent patterns (non-retryable)
    for pattern in PERMANENT_PATTERNS {
        if lower.contains(pattern) {
            return false;
        }
    }

    // Then check if it matches retryable patterns
    for pattern in RETRYABLE_PATTERNS {
        if lower.contains(pattern) {
            return true;
        }
    }

    // Default: not retryable (fail fast on unknown errors)
    false
}

/// Execute an async operation with retry logic
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation` - A factory function that produces the async operation to retry
///
/// # Returns
/// The result of the operation, or the last error if all retries failed
pub async fn execute_with_retry<F, Fut, T>(
    config: &RetryConfig,
    operation: F,
) -> Result<T, anyhow::Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, anyhow::Error>>,
{
    let mut last_error: Option<anyhow::Error> = None;

    // Total attempts = 1 initial + max_retries
    let total_attempts = 1 + config.max_retries;

    for attempt in 0..total_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                let error_msg = err.to_string();
                let is_retryable = is_retryable_error(&error_msg);

                if !is_retryable {
                    // Non-retryable error: fail immediately
                    tracing::debug!(
                        "Non-retryable error on attempt {}: {}",
                        attempt + 1,
                        error_msg
                    );
                    return Err(err);
                }

                // Check if we have retries left
                let retries_remaining = total_attempts.saturating_sub(attempt + 1);
                if retries_remaining == 0 {
                    last_error = Some(err);
                    break;
                }

                // Calculate delay and wait
                let delay = if config.jitter_enabled {
                    config.delay_for_attempt_with_jitter(attempt)
                } else {
                    config.delay_for_attempt(attempt)
                };

                tracing::debug!(
                    "Retryable error on attempt {} ({}ms delay, {} retries left): {}",
                    attempt + 1,
                    delay.as_millis(),
                    retries_remaining,
                    error_msg
                );

                tokio::time::sleep(delay).await;
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Retry exhausted with no error captured")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_default() {
        assert_eq!(RetryPolicy::default(), RetryPolicy::ExponentialBackoff);
    }

    #[test]
    fn test_retryable_patterns() {
        assert!(is_retryable_error("ETIMEDOUT"));
        assert!(is_retryable_error("Connection reset by peer"));
        assert!(!is_retryable_error("some random error"));
    }

    #[test]
    fn test_permanent_patterns_take_precedence() {
        // Even if "timed out" could match, "permission denied" is permanent
        assert!(!is_retryable_error("Permission denied while timed out"));
    }
}
