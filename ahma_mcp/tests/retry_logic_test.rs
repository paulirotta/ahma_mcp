//! TDD Tests for Retry Logic with Exponential Backoff
//!
//! These tests define the expected behavior for the retry system before implementation.
//! Following TDD: Write tests first, then implement to make them pass.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Test: RetryConfig should have sensible defaults
#[test]
fn test_retry_config_defaults() {
    use ahma_mcp::retry::{RetryConfig, RetryPolicy};

    let config = RetryConfig::default();

    assert_eq!(config.max_retries, 3, "Default max retries should be 3");
    assert_eq!(
        config.initial_delay,
        Duration::from_millis(100),
        "Initial delay should be 100ms"
    );
    assert_eq!(
        config.max_delay,
        Duration::from_secs(5),
        "Max delay should be 5 seconds"
    );
    assert_eq!(config.backoff_factor, 2.0, "Backoff factor should be 2.0");
    assert!(
        matches!(config.policy, RetryPolicy::ExponentialBackoff),
        "Default policy should be exponential backoff"
    );
}

/// Test: RetryConfig builder pattern for customization
#[test]
fn test_retry_config_builder() {
    use ahma_mcp::retry::RetryConfig;

    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(200))
        .with_max_delay(Duration::from_secs(10))
        .with_backoff_factor(3.0);

    assert_eq!(config.max_retries, 5);
    assert_eq!(config.initial_delay, Duration::from_millis(200));
    assert_eq!(config.max_delay, Duration::from_secs(10));
    assert_eq!(config.backoff_factor, 3.0);
}

/// Test: Exponential backoff delay calculation
#[test]
fn test_exponential_backoff_delay_calculation() {
    use ahma_mcp::retry::RetryConfig;

    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_backoff_factor(2.0)
        .with_max_delay(Duration::from_secs(10));

    // Delay for attempt 0 (first retry): 100ms
    assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
    // Delay for attempt 1: 200ms (100 * 2^1)
    assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
    // Delay for attempt 2: 400ms (100 * 2^2)
    assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
    // Delay for attempt 3: 800ms (100 * 2^3)
    assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800));
}

/// Test: Delay should be capped at max_delay
#[test]
fn test_delay_capped_at_max() {
    use ahma_mcp::retry::RetryConfig;

    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_secs(1))
        .with_backoff_factor(10.0)
        .with_max_delay(Duration::from_secs(5));

    // Attempt 2 would be 1 * 10^2 = 100s, but capped at 5s
    assert_eq!(config.delay_for_attempt(2), Duration::from_secs(5));
}

/// Test: RetryableError classification - transient errors should be retryable
#[test]
fn test_is_retryable_error_transient() {
    use ahma_mcp::retry::is_retryable_error;

    // Transient errors that should be retried
    assert!(is_retryable_error("Connection timed out"));
    assert!(is_retryable_error("ETIMEDOUT: connection timed out"));
    assert!(is_retryable_error("Resource temporarily unavailable"));
    assert!(is_retryable_error("EAGAIN: resource busy"));
    assert!(is_retryable_error("Connection reset by peer"));
    assert!(is_retryable_error("Broken pipe"));
    assert!(is_retryable_error("Network is unreachable"));
}

/// Test: Non-retryable errors should not be retried
#[test]
fn test_is_retryable_error_permanent() {
    use ahma_mcp::retry::is_retryable_error;

    // Permanent errors that should NOT be retried
    assert!(!is_retryable_error("Permission denied"));
    assert!(!is_retryable_error("No such file or directory"));
    assert!(!is_retryable_error("Command not found"));
    assert!(!is_retryable_error("syntax error near unexpected token"));
    assert!(!is_retryable_error("Invalid argument"));
}

/// Test: Successful operation should not retry
#[tokio::test]
async fn test_no_retry_on_success() {
    use ahma_mcp::retry::{RetryConfig, execute_with_retry};

    let attempt_count = Arc::new(AtomicU32::new(0));
    let counter = attempt_count.clone();

    let config = RetryConfig::default();

    let result = execute_with_retry(&config, || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok::<_, anyhow::Error>("success".to_string())
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(
        attempt_count.load(Ordering::SeqCst),
        1,
        "Should only execute once on success"
    );
}

/// Test: Retryable error should trigger retries up to max
#[tokio::test]
async fn test_retry_on_transient_error() {
    use ahma_mcp::retry::{RetryConfig, execute_with_retry};

    let attempt_count = Arc::new(AtomicU32::new(0));
    let counter = attempt_count.clone();

    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(10)); // Fast for testing

    let result = execute_with_retry(&config, || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<String, _>(anyhow::anyhow!("Connection timed out"))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(
        attempt_count.load(Ordering::SeqCst),
        4, // 1 initial + 3 retries
        "Should retry 3 times after initial failure"
    );
}

/// Test: Non-retryable error should fail immediately without retrying
#[tokio::test]
async fn test_no_retry_on_permanent_error() {
    use ahma_mcp::retry::{RetryConfig, execute_with_retry};

    let attempt_count = Arc::new(AtomicU32::new(0));
    let counter = attempt_count.clone();

    let config = RetryConfig::new()
        .with_max_retries(3)
        .with_initial_delay(Duration::from_millis(10));

    let result = execute_with_retry(&config, || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<String, _>(anyhow::anyhow!("Permission denied"))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(
        attempt_count.load(Ordering::SeqCst),
        1,
        "Should fail immediately on non-retryable error"
    );
}

/// Test: Retry eventually succeeds
#[tokio::test]
async fn test_retry_eventually_succeeds() {
    use ahma_mcp::retry::{RetryConfig, execute_with_retry};

    let attempt_count = Arc::new(AtomicU32::new(0));
    let counter = attempt_count.clone();

    let config = RetryConfig::new()
        .with_max_retries(5)
        .with_initial_delay(Duration::from_millis(10));

    let result = execute_with_retry(&config, || {
        let counter = counter.clone();
        async move {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                // Fail first two attempts with retryable error
                Err(anyhow::anyhow!("Connection timed out"))
            } else {
                // Succeed on third attempt
                Ok("success".to_string())
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(
        attempt_count.load(Ordering::SeqCst),
        3, // 2 failures + 1 success
        "Should succeed on third attempt"
    );
}

/// Test: Disabled retry (max_retries = 0) executes once
#[tokio::test]
async fn test_disabled_retry() {
    use ahma_mcp::retry::{RetryConfig, execute_with_retry};

    let attempt_count = Arc::new(AtomicU32::new(0));
    let counter = attempt_count.clone();

    let config = RetryConfig::new().with_max_retries(0);

    let result = execute_with_retry(&config, || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Err::<String, _>(anyhow::anyhow!("Connection timed out"))
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(
        attempt_count.load(Ordering::SeqCst),
        1,
        "Should execute exactly once when retries disabled"
    );
}

/// Test: RetryConfig is Clone and Debug
#[test]
fn test_retry_config_traits() {
    use ahma_mcp::retry::RetryConfig;

    let config = RetryConfig::default();
    let cloned = config.clone();

    assert_eq!(config.max_retries, cloned.max_retries);
    assert_eq!(config.initial_delay, cloned.initial_delay);

    // Debug should not panic
    let debug_str = format!("{:?}", config);
    assert!(!debug_str.is_empty());
}

/// Test: Jitter adds randomization to delays (optional feature)
#[test]
fn test_jitter_adds_variance() {
    use ahma_mcp::retry::RetryConfig;

    let config = RetryConfig::new()
        .with_initial_delay(Duration::from_millis(100))
        .with_jitter(true);

    // With jitter, delays should have some variance
    // We can't test exact values, but we can verify it doesn't panic
    // and returns reasonable values
    let delay1 = config.delay_for_attempt_with_jitter(1);
    let delay2 = config.delay_for_attempt_with_jitter(1);

    // Attempt 1 with backoff factor 2.0 = 200ms base
    // With ±50% jitter, range is 100ms to 300ms
    assert!(
        delay1 >= Duration::from_millis(50),
        "delay1 ({:?}) should be >= 50ms",
        delay1
    );
    assert!(
        delay1 <= Duration::from_millis(400),
        "delay1 ({:?}) should be <= 400ms",
        delay1
    );
    assert!(
        delay2 >= Duration::from_millis(50),
        "delay2 ({:?}) should be >= 50ms",
        delay2
    );
    assert!(
        delay2 <= Duration::from_millis(400),
        "delay2 ({:?}) should be <= 400ms",
        delay2
    );
}
