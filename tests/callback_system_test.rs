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
async fn test_progress_update_terminal_state() {
    let started = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "cargo build".to_string(),
        description: "Building".to_string(),
    };
    assert!(!started.is_terminal());

    let progress = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Working".to_string(),
        percentage: Some(50.0),
        current_step: None,
    };
    assert!(!progress.is_terminal());

    let output = ProgressUpdate::Output {
        operation_id: "op_123".to_string(),
        line: "Building crate".to_string(),
        is_stderr: false,
    };
    assert!(!output.is_terminal());

    let completed = ProgressUpdate::Completed {
        operation_id: "op_123".to_string(),
        message: "Done".to_string(),
        duration_ms: 1000,
    };
    assert!(completed.is_terminal());

    let failed = ProgressUpdate::Failed {
        operation_id: "op_123".to_string(),
        error: "Error".to_string(),
        duration_ms: 1000,
    };
    assert!(failed.is_terminal());

    let cancelled = ProgressUpdate::Cancelled {
        operation_id: "op_123".to_string(),
        message: "Cancelled".to_string(),
        duration_ms: 1000,
    };
    assert!(cancelled.is_terminal());

    let final_result = ProgressUpdate::FinalResult {
        operation_id: "op_123".to_string(),
        command: "cargo test".to_string(),
        description: "Testing".to_string(),
        working_directory: "/test".to_string(),
        success: true,
        duration_ms: 1000,
        full_output: "Success".to_string(),
    };
    assert!(final_result.is_terminal());
}

#[tokio::test]
async fn test_progress_update_success_failure() {
    let completed = ProgressUpdate::Completed {
        operation_id: "op_123".to_string(),
        message: "Done".to_string(),
        duration_ms: 1000,
    };
    assert!(completed.is_success());
    assert!(!completed.is_failure());

    let failed = ProgressUpdate::Failed {
        operation_id: "op_123".to_string(),
        error: "Error".to_string(),
        duration_ms: 1000,
    };
    assert!(!failed.is_success());
    assert!(failed.is_failure());

    let cancelled = ProgressUpdate::Cancelled {
        operation_id: "op_123".to_string(),
        message: "Cancelled".to_string(),
        duration_ms: 1000,
    };
    assert!(!cancelled.is_success());
    assert!(cancelled.is_failure());

    let final_success = ProgressUpdate::FinalResult {
        operation_id: "op_123".to_string(),
        command: "cmd".to_string(),
        description: "desc".to_string(),
        working_directory: "/test".to_string(),
        success: true,
        duration_ms: 1000,
        full_output: "output".to_string(),
    };
    assert!(final_success.is_success());
    assert!(!final_success.is_failure());

    let final_failure = ProgressUpdate::FinalResult {
        operation_id: "op_123".to_string(),
        command: "cmd".to_string(),
        description: "desc".to_string(),
        working_directory: "/test".to_string(),
        success: false,
        duration_ms: 1000,
        full_output: "error output".to_string(),
    };
    assert!(!final_failure.is_success());
    assert!(final_failure.is_failure());

    let started = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "cmd".to_string(),
        description: "desc".to_string(),
    };
    assert!(!started.is_success());
    assert!(!started.is_failure());
}

#[tokio::test]
async fn test_progress_update_operation_id() {
    let updates = vec![
        ProgressUpdate::Started {
            operation_id: "op_123".to_string(),
            command: "cmd".to_string(),
            description: "desc".to_string(),
        },
        ProgressUpdate::Progress {
            operation_id: "op_456".to_string(),
            message: "msg".to_string(),
            percentage: None,
            current_step: None,
        },
        ProgressUpdate::Output {
            operation_id: "op_789".to_string(),
            line: "line".to_string(),
            is_stderr: false,
        },
        ProgressUpdate::Completed {
            operation_id: "op_abc".to_string(),
            message: "msg".to_string(),
            duration_ms: 1000,
        },
        ProgressUpdate::Failed {
            operation_id: "op_def".to_string(),
            error: "error".to_string(),
            duration_ms: 1000,
        },
        ProgressUpdate::Cancelled {
            operation_id: "op_ghi".to_string(),
            message: "msg".to_string(),
            duration_ms: 1000,
        },
        ProgressUpdate::FinalResult {
            operation_id: "op_jkl".to_string(),
            command: "cmd".to_string(),
            description: "desc".to_string(),
            working_directory: "/test".to_string(),
            success: true,
            duration_ms: 1000,
            full_output: "output".to_string(),
        },
    ];

    let expected_ids = vec![
        "op_123", "op_456", "op_789", "op_abc", "op_def", "op_ghi", "op_jkl",
    ];

    for (update, expected_id) in updates.iter().zip(expected_ids.iter()) {
        assert_eq!(update.operation_id(), *expected_id);
    }
}

#[tokio::test]
async fn test_progress_update_duration() {
    let started = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "cmd".to_string(),
        description: "desc".to_string(),
    };
    assert_eq!(started.duration_ms(), None);

    let completed = ProgressUpdate::Completed {
        operation_id: "op_123".to_string(),
        message: "msg".to_string(),
        duration_ms: 5000,
    };
    assert_eq!(completed.duration_ms(), Some(5000));

    let failed = ProgressUpdate::Failed {
        operation_id: "op_123".to_string(),
        error: "error".to_string(),
        duration_ms: 3000,
    };
    assert_eq!(failed.duration_ms(), Some(3000));

    let cancelled = ProgressUpdate::Cancelled {
        operation_id: "op_123".to_string(),
        message: "msg".to_string(),
        duration_ms: 1500,
    };
    assert_eq!(cancelled.duration_ms(), Some(1500));

    let final_result = ProgressUpdate::FinalResult {
        operation_id: "op_123".to_string(),
        command: "cmd".to_string(),
        description: "desc".to_string(),
        working_directory: "/test".to_string(),
        success: true,
        duration_ms: 8000,
        full_output: "output".to_string(),
    };
    assert_eq!(final_result.duration_ms(), Some(8000));
}

#[tokio::test]
async fn test_progress_update_variant_name() {
    let updates_and_names = vec![
        (
            ProgressUpdate::Started {
                operation_id: "op".to_string(),
                command: "cmd".to_string(),
                description: "desc".to_string(),
            },
            "Started",
        ),
        (
            ProgressUpdate::Progress {
                operation_id: "op".to_string(),
                message: "msg".to_string(),
                percentage: None,
                current_step: None,
            },
            "Progress",
        ),
        (
            ProgressUpdate::Output {
                operation_id: "op".to_string(),
                line: "line".to_string(),
                is_stderr: false,
            },
            "Output",
        ),
        (
            ProgressUpdate::Completed {
                operation_id: "op".to_string(),
                message: "msg".to_string(),
                duration_ms: 1000,
            },
            "Completed",
        ),
        (
            ProgressUpdate::Failed {
                operation_id: "op".to_string(),
                error: "error".to_string(),
                duration_ms: 1000,
            },
            "Failed",
        ),
        (
            ProgressUpdate::Cancelled {
                operation_id: "op".to_string(),
                message: "msg".to_string(),
                duration_ms: 1000,
            },
            "Cancelled",
        ),
        (
            ProgressUpdate::FinalResult {
                operation_id: "op".to_string(),
                command: "cmd".to_string(),
                description: "desc".to_string(),
                working_directory: "/test".to_string(),
                success: true,
                duration_ms: 1000,
                full_output: "output".to_string(),
            },
            "FinalResult",
        ),
    ];

    for (update, expected_name) in updates_and_names {
        assert_eq!(update.variant_name(), expected_name);
    }
}

#[tokio::test]
async fn test_callback_error_properties() {
    let send_failed = CallbackError::SendFailed("Send error message".to_string());
    assert!(send_failed.is_recoverable());
    assert!(!send_failed.is_user_initiated());
    assert_eq!(send_failed.error_code(), "SEND_FAILED");
    assert_eq!(send_failed.severity(), "ERROR");
    assert_eq!(send_failed.message_detail(), Some("Send error message"));

    let disconnected = CallbackError::Disconnected;
    assert!(!disconnected.is_recoverable());
    assert!(!disconnected.is_user_initiated());
    assert_eq!(disconnected.error_code(), "DISCONNECTED");
    assert_eq!(disconnected.severity(), "ERROR");
    assert_eq!(disconnected.message_detail(), None);

    let cancelled = CallbackError::Cancelled;
    assert!(!cancelled.is_recoverable());
    assert!(cancelled.is_user_initiated());
    assert_eq!(cancelled.error_code(), "CANCELLED");
    assert_eq!(cancelled.severity(), "WARN");
    assert_eq!(cancelled.message_detail(), None);

    let timeout = CallbackError::Timeout("Timeout details".to_string());
    assert!(timeout.is_recoverable());
    assert!(!timeout.is_user_initiated());
    assert_eq!(timeout.error_code(), "TIMEOUT");
    assert_eq!(timeout.severity(), "ERROR");
    assert_eq!(timeout.message_detail(), Some("Timeout details"));
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

        assert_eq!(received.operation_id(), expected_update.operation_id());
        assert_eq!(received.variant_name(), expected_update.variant_name());
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
    assert_eq!(received.operation_id(), update.operation_id());
}
