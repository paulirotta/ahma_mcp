use ahma_mcp::callback_system::{
    CallbackError, CallbackSender, ChannelCallbackSender, LoggingCallbackSender,
    NoOpCallbackSender, ProgressUpdate, channel_callback, logging_callback, no_callback,
};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_progress_update_display() {
    let started = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "cargo build".to_string(),
        description: "Building the project".to_string(),
    };
    assert_eq!(
        format!("{}", started),
        "[op_123] Started: cargo build - Building the project"
    );

    let progress = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Compiling dependencies".to_string(),
        percentage: Some(75.5),
        current_step: Some("Stage 2".to_string()),
    };
    assert_eq!(
        format!("{}", progress),
        "[op_123] Progress (75.5%): Compiling dependencies [Stage 2]"
    );

    let progress_no_percentage = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Processing".to_string(),
        percentage: None,
        current_step: None,
    };
    assert_eq!(
        format!("{}", progress_no_percentage),
        "[op_123] Progress: Processing"
    );

    let output = ProgressUpdate::Output {
        operation_id: "op_123".to_string(),
        line: "Finished release [optimized] target(s)".to_string(),
        is_stderr: false,
    };
    assert_eq!(
        format!("{}", output),
        "[op_123] stdout: Finished release [optimized] target(s)"
    );

    let output_stderr = ProgressUpdate::Output {
        operation_id: "op_123".to_string(),
        line: "warning: unused variable".to_string(),
        is_stderr: true,
    };
    assert_eq!(
        format!("{}", output_stderr),
        "[op_123] stderr: warning: unused variable"
    );

    let completed = ProgressUpdate::Completed {
        operation_id: "op_123".to_string(),
        message: "Build successful".to_string(),
        duration_ms: 5000,
    };
    assert_eq!(
        format!("{}", completed),
        "[op_123] Completed in 5000ms: Build successful"
    );

    let failed = ProgressUpdate::Failed {
        operation_id: "op_123".to_string(),
        error: "Compilation error".to_string(),
        duration_ms: 2500,
    };
    assert_eq!(
        format!("{}", failed),
        "[op_123] Failed after 2500ms: Compilation error"
    );

    let cancelled = ProgressUpdate::Cancelled {
        operation_id: "op_123".to_string(),
        message: "User cancelled build".to_string(),
        duration_ms: 1500,
    };
    assert_eq!(
        format!("{}", cancelled),
        "[op_123] CANCELLED after 1500ms: User cancelled build"
    );

    let final_result = ProgressUpdate::FinalResult {
        operation_id: "op_123".to_string(),
        command: "cargo test".to_string(),
        description: "Running tests".to_string(),
        working_directory: "/home/user/project".to_string(),
        success: true,
        duration_ms: 8000,
        full_output: "All tests passed".to_string(),
    };
    assert_eq!(
        format!("{}", final_result),
        "[op_123] COMPLETED: cargo test\nAll tests passed"
    );

    let final_result_failed = ProgressUpdate::FinalResult {
        operation_id: "op_456".to_string(),
        command: "cargo test".to_string(),
        description: "Running tests".to_string(),
        working_directory: "/home/user/project".to_string(),
        success: false,
        duration_ms: 3000,
        full_output: "2 tests failed".to_string(),
    };
    assert_eq!(
        format!("{}", final_result_failed),
        "[op_456] FAILED: cargo test\n2 tests failed"
    );
}

#[tokio::test]
async fn test_logging_callback_sender() {
    let callback = LoggingCallbackSender::new("test_operation".to_string());

    let update = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "cargo build".to_string(),
        description: "Building".to_string(),
    };

    // Should not fail - logs are sent to tracing
    assert!(callback.send_progress(update).await.is_ok());
    assert!(!callback.should_cancel().await);
}

#[tokio::test]
async fn test_no_op_callback_sender() {
    let callback = NoOpCallbackSender;

    let update = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Testing".to_string(),
        percentage: Some(100.0),
        current_step: None,
    };

    assert!(callback.send_progress(update).await.is_ok());
    assert!(!callback.should_cancel().await);
}

#[tokio::test]
async fn test_channel_callback_sender_direct() {
    let (sender, _receiver) = mpsc::unbounded_channel();
    let token = CancellationToken::new();
    let callback = ChannelCallbackSender::new(sender, token.clone());

    let update = ProgressUpdate::Output {
        operation_id: "op_123".to_string(),
        line: "Test output".to_string(),
        is_stderr: true,
    };

    assert!(callback.send_progress(update).await.is_ok());
    assert!(!callback.should_cancel().await);

    token.cancel();
    assert!(callback.should_cancel().await);
}

#[tokio::test]
async fn test_channel_callback_disconnected() {
    let (sender, receiver) = mpsc::unbounded_channel();
    let token = CancellationToken::new();
    let callback = ChannelCallbackSender::new(sender, token);

    // Drop receiver to simulate disconnection
    drop(receiver);

    let update = ProgressUpdate::Completed {
        operation_id: "op_123".to_string(),
        message: "Done".to_string(),
        duration_ms: 1000,
    };

    // Should fail with Disconnected error
    let result = callback.send_progress(update).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CallbackError::Disconnected));
}

#[tokio::test]
async fn test_callback_send_batch() {
    let token = CancellationToken::new();
    let (callback, mut receiver) = channel_callback(token);

    let updates = vec![
        ProgressUpdate::Started {
            operation_id: "op_123".to_string(),
            command: "cargo build".to_string(),
            description: "Building".to_string(),
        },
        ProgressUpdate::Progress {
            operation_id: "op_123".to_string(),
            message: "Compiling".to_string(),
            percentage: Some(50.0),
            current_step: None,
        },
        ProgressUpdate::Completed {
            operation_id: "op_123".to_string(),
            message: "Done".to_string(),
            duration_ms: 2000,
        },
    ];

    assert!(callback.send_batch(updates.clone()).await.is_ok());

    // Verify all updates were received
    for expected_update in updates {
        let received = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(format!("{received}"), format!("{expected_update}"));
    }
}

#[tokio::test]
async fn test_utility_functions() {
    // Test no_callback
    let callback = no_callback();
    let update = ProgressUpdate::Started {
        operation_id: "test".to_string(),
        command: "test".to_string(),
        description: "test".to_string(),
    };
    assert!(callback.send_progress(update).await.is_ok());

    // Test logging_callback
    let callback = logging_callback("test_op".to_string());
    let update = ProgressUpdate::Progress {
        operation_id: "test".to_string(),
        message: "test".to_string(),
        percentage: None,
        current_step: None,
    };
    assert!(callback.send_progress(update).await.is_ok());

    // Test channel_callback
    let token = CancellationToken::new();
    let (callback, mut receiver) = channel_callback(token);
    let update = ProgressUpdate::Completed {
        operation_id: "test".to_string(),
        message: "test".to_string(),
        duration_ms: 1000,
    };
    assert!(callback.send_progress(update.clone()).await.is_ok());

    let received = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(format!("{received}"), format!("{update}"));
}
// =========================================================================
// Tests for format_cancellation_message helper
// =========================================================================

#[test]
fn test_format_cancellation_message_canceled_canceled() {
    use ahma_mcp::callback_system::format_cancellation_message;

    // The infamous "Canceled: canceled" pattern from VS Code
    let result = format_cancellation_message("Canceled: canceled", Some("cargo"), Some("op_123"));
    assert!(
        result.contains("Operation was cancelled (source: unknown)"),
        "Expected unknown-source cancellation message, got: {}",
        result
    );
    assert!(
        result.contains("Raw: Canceled: canceled"),
        "Expected raw cancellation message to be preserved, got: {}",
        result
    );
    assert!(
        result.contains("cargo"),
        "Expected tool name in message, got: {}",
        result
    );
    assert!(
        result.contains("op_123"),
        "Expected operation ID in message, got: {}",
        result
    );
    assert!(
        result.contains("Suggestions"),
        "Expected actionable suggestions, got: {}",
        result
    );
}

#[test]
fn test_format_cancellation_message_lowercase_canceled() {
    use ahma_mcp::callback_system::format_cancellation_message;

    // Just "canceled" without the prefix
    let result = format_cancellation_message("canceled", None, None);
    assert!(
        result.contains("Operation was cancelled (source: unknown)"),
        "Expected unknown-source cancellation message, got: {}",
        result
    );
    assert!(
        result.contains("Raw: canceled"),
        "Expected raw cancellation message to be preserved, got: {}",
        result
    );
}

#[test]
fn test_format_cancellation_message_task_cancelled() {
    use ahma_mcp::callback_system::format_cancellation_message;

    // rmcp ServiceError::Cancelled format
    let result =
        format_cancellation_message("task cancelled for reason timeout", Some("clippy"), None);
    assert!(
        result.contains("MCP cancellation received"),
        "Expected MCP cancellation message, got: {}",
        result
    );
    assert!(
        result.contains("Raw: task cancelled for reason timeout"),
        "Expected raw cancellation message to be preserved, got: {}",
        result
    );
    assert!(
        result.contains("clippy"),
        "Expected tool name in message, got: {}",
        result
    );
}

#[test]
fn test_format_cancellation_message_timeout() {
    use ahma_mcp::callback_system::format_cancellation_message;

    let result = format_cancellation_message("Operation timed out after 30s", Some("build"), None);
    assert!(
        result.contains("timed out"),
        "Expected timeout message, got: {}",
        result
    );
}

#[test]
fn test_format_cancellation_message_non_cancellation() {
    use ahma_mcp::callback_system::format_cancellation_message;

    // Non-cancellation error should be returned as-is
    let original = "Command failed with exit code 1";
    let result = format_cancellation_message(original, Some("test"), Some("op_456"));
    assert_eq!(
        result, original,
        "Non-cancellation messages should be returned unchanged"
    );
}

#[test]
fn test_format_cancellation_message_user_initiated() {
    use ahma_mcp::callback_system::format_cancellation_message;

    let result = format_cancellation_message("User requested cancellation", None, None);
    assert!(
        result.contains("User-initiated cancellation"),
        "Expected user cancellation message, got: {}",
        result
    );
}
