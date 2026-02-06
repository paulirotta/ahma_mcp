use std::time::Duration;
use tokio::sync::mpsc;

/// Default timeout for CI-resilient notification waiting (10 seconds).
pub const CI_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Wait for a notification on an mpsc channel with timeout.
///
/// Returns `Some(notification)` if received within timeout, `None` otherwise.
pub async fn wait_for_notification<T>(rx: &mut mpsc::Receiver<T>, timeout: Duration) -> Option<T>
where
    T: Send,
{
    tokio::time::timeout(timeout, rx.recv())
        .await
        .ok()
        .flatten()
}

/// Wait for a notification matching a predicate with timeout.
///
/// Keeps receiving notifications until one matches or timeout expires.
/// Non-matching notifications are discarded.
pub async fn wait_for_notification_matching<T, F>(
    rx: &mut mpsc::Receiver<T>,
    timeout: Duration,
    predicate: F,
) -> Option<T>
where
    T: Send,
    F: Fn(&T) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(item)) if predicate(&item) => return Some(item),
            Ok(Some(_)) => continue, // Non-matching, try again
            Ok(None) => return None, // Channel closed
            Err(_) => return None,   // Timeout
        }
    }
}
