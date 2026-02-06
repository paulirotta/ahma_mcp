use std::collections::HashSet;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;

/// Default CI-friendly timeout for concurrent operations.
pub const CI_DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Generous timeout for heavy operations (compilation, network, etc.)
pub const CI_HEAVY_TIMEOUT: Duration = Duration::from_secs(120);

/// Short timeout for quick sanity checks
pub const CI_QUICK_TIMEOUT: Duration = Duration::from_secs(5);

/// Wait for an async condition to become true, polling at a fixed interval.
/// Returns true if the condition succeeds within the timeout.
pub async fn wait_for_condition<F, Fut>(
    timeout: Duration,
    interval: Duration,
    mut condition: F,
) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = tokio::time::Instant::now();
    loop {
        if condition().await {
            return true;
        }

        if start.elapsed() >= timeout {
            return false;
        }

        tokio::time::sleep(interval).await;
    }
}

/// Wait for an operation to reach a terminal state, checking both active and completed history.
pub async fn wait_for_operation_terminal(
    monitor: &crate::operation_monitor::OperationMonitor,
    operation_id: &str,
    timeout: Duration,
    interval: Duration,
) -> bool {
    wait_for_condition(timeout, interval, || {
        let monitor = monitor.clone();
        let operation_id = operation_id.to_string();
        async move {
            if let Some(op) = monitor.get_operation(&operation_id).await {
                return op.state.is_terminal();
            }

            let completed = monitor.get_completed_operations().await;
            completed.iter().any(|op| op.id == operation_id)
        }
    })
    .await
}

/// Spawn multiple tasks that all start simultaneously using a barrier.
pub async fn spawn_tasks_with_barrier<F, Fut, T>(count: usize, task_fn: F) -> Vec<T>
where
    F: Fn(usize) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let barrier = Arc::new(Barrier::new(count));
    let task_fn = Arc::new(task_fn);

    let handles: Vec<_> = (0..count)
        .map(|i| {
            let barrier = barrier.clone();
            let task_fn = task_fn.clone();
            tokio::spawn(async move {
                // All tasks wait here until everyone is ready
                barrier.wait().await;
                task_fn(i).await
            })
        })
        .collect();

    let mut results = Vec::with_capacity(count);
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }
    results
}

/// Collect results from concurrent operations with a deadline.
pub async fn collect_results_with_deadline<T>(
    rx: &mut tokio::sync::mpsc::Receiver<T>,
    expected_count: usize,
    timeout: Duration,
) -> Vec<T>
where
    T: Send,
{
    let mut results = Vec::with_capacity(expected_count);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if results.len() >= expected_count {
            break;
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(result)) => results.push(result),
            Ok(None) => break, // Channel closed
            Err(_) => break,   // Timeout
        }
    }

    results
}

/// Assert that all items in a collection are unique.
pub fn assert_all_unique<T: Eq + Hash + std::fmt::Debug>(items: &[T]) {
    let mut seen = HashSet::new();
    for item in items {
        assert!(
            seen.insert(item),
            "Duplicate item detected: {:?}. This indicates a race condition or notification bug.",
            item
        );
    }
}

/// Assert that a collection has no duplicates when mapped by a key function.
pub fn assert_unique_by<T, K, F>(items: &[T], key_fn: F)
where
    K: Eq + Hash + std::fmt::Debug,
    F: Fn(&T) -> K,
{
    let mut seen = HashSet::new();
    for item in items {
        let key = key_fn(item);
        assert!(
            seen.insert(key),
            "Duplicate key detected. This indicates a race condition or notification bug."
        );
    }
}

/// Wrap a future with a CI-appropriate timeout.
pub async fn with_ci_timeout<T, Fut>(
    operation_name: &str,
    timeout: Duration,
    fut: Fut,
) -> anyhow::Result<T>
where
    Fut: Future<Output = T>,
{
    tokio::time::timeout(timeout, fut).await.map_err(|_| {
        anyhow::anyhow!(
            "CI timeout: '{}' did not complete within {:?}. \
             This may indicate a deadlock, resource starvation, or slow CI environment.",
            operation_name,
            timeout
        )
    })
}

/// Wait for a condition with exponential backoff polling.
pub async fn wait_with_backoff<F, Fut>(
    description: &str,
    timeout: Duration,
    mut condition: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = tokio::time::Instant::now();
    let mut interval = Duration::from_millis(10);
    let max_interval = Duration::from_millis(500);

    loop {
        if condition().await {
            return Ok(());
        }

        if start.elapsed() >= timeout {
            return Err(anyhow::anyhow!(
                "Condition '{}' not met within {:?}. This may indicate a bug or slow CI.",
                description,
                timeout
            ));
        }

        tokio::time::sleep(interval).await;
        interval = (interval * 2).min(max_interval);
    }
}

/// Spawn concurrent tasks with a bounded limit to prevent resource exhaustion.
pub async fn spawn_bounded_concurrent<I, T, F, Fut, R>(
    items: I,
    concurrency_limit: usize,
    task_fn: F,
) -> Vec<R>
where
    I: IntoIterator<Item = T>,
    T: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    use tokio::sync::Semaphore;

    let semaphore = Arc::new(Semaphore::new(concurrency_limit));
    let task_fn = Arc::new(task_fn);

    let handles: Vec<_> = items
        .into_iter()
        .map(|item| {
            let semaphore = semaphore.clone();
            let task_fn = task_fn.clone();
            tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                task_fn(item).await
            })
        })
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }
    results
}
