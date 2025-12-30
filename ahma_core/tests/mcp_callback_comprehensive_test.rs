//! Tests for mcp_callback.rs - MCP callback sender implementation
//!
//! This module tests the McpCallbackSender which translates internal ProgressUpdate
//! events into MCP-compliant ProgressNotificationParam messages.
//!
//! Since McpCallbackSender requires a real Peer<RoleServer> which is difficult to mock,
//! we test the callback behavior indirectly through the callback_system trait and
//! verify the ProgressUpdate formatting logic.

use ahma_core::callback_system::{
    CallbackError, CallbackSender, ProgressUpdate, format_cancellation_message,
};
use ahma_core::client_type::McpClientType;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// Mock callback sender for testing callback behavior
#[derive(Clone)]
struct MockCallbackSender {
    received_updates: Arc<Mutex<Vec<ProgressUpdate>>>,
    should_fail: bool,
    client_type: McpClientType,
}

impl MockCallbackSender {
    fn new() -> Self {
        Self {
            received_updates: Arc::new(Mutex::new(Vec::new())),
            should_fail: false,
            client_type: McpClientType::Unknown,
        }
    }

    fn with_client_type(mut self, client_type: McpClientType) -> Self {
        self.client_type = client_type;
        self
    }

    fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }

    fn get_updates(&self) -> Vec<ProgressUpdate> {
        self.received_updates.lock().unwrap().clone()
    }
}

#[async_trait]
impl CallbackSender for MockCallbackSender {
    async fn send_progress(&self, update: ProgressUpdate) -> Result<(), CallbackError> {
        if self.should_fail {
            return Err(CallbackError::SendFailed("Mock failure".to_string()));
        }

        // Simulate client type filtering (like McpCallbackSender does)
        if !self.client_type.supports_progress() {
            // Skip but don't record - simulating Cursor behavior
            return Ok(());
        }

        self.received_updates.lock().unwrap().push(update);
        Ok(())
    }

    async fn should_cancel(&self) -> bool {
        false
    }
}

// ============= ProgressUpdate Started Tests =============

#[tokio::test]
async fn test_progress_update_started() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "cargo build".to_string(),
        description: "Building project".to_string(),
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    assert_eq!(updates.len(), 1);

    match &updates[0] {
        ProgressUpdate::Started {
            operation_id,
            command,
            description,
        } => {
            assert_eq!(operation_id, "op_123");
            assert_eq!(command, "cargo build");
            assert_eq!(description, "Building project");
        }
        _ => panic!("Expected Started update"),
    }
}

// ============= ProgressUpdate Progress Tests =============

#[tokio::test]
async fn test_progress_update_with_percentage() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Compiling crate".to_string(),
        percentage: Some(50.0),
        current_step: None,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    assert_eq!(updates.len(), 1);

    match &updates[0] {
        ProgressUpdate::Progress {
            percentage,
            message,
            ..
        } => {
            assert_eq!(*percentage, Some(50.0));
            assert_eq!(message, "Compiling crate");
        }
        _ => panic!("Expected Progress update"),
    }
}

#[tokio::test]
async fn test_progress_update_without_percentage() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Processing...".to_string(),
        percentage: None,
        current_step: None,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::Progress { percentage, .. } => {
            assert!(percentage.is_none());
        }
        _ => panic!("Expected Progress update"),
    }
}

// ============= ProgressUpdate Output Tests =============

#[tokio::test]
async fn test_progress_update_stdout_output() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Output {
        operation_id: "op_123".to_string(),
        line: "Compiling ahma_core v0.4.0".to_string(),
        is_stderr: false,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::Output {
            line, is_stderr, ..
        } => {
            assert_eq!(line, "Compiling ahma_core v0.4.0");
            assert!(!is_stderr);
        }
        _ => panic!("Expected Output update"),
    }
}

#[tokio::test]
async fn test_progress_update_stderr_output() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Output {
        operation_id: "op_123".to_string(),
        line: "warning: unused variable".to_string(),
        is_stderr: true,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::Output {
            line, is_stderr, ..
        } => {
            assert_eq!(line, "warning: unused variable");
            assert!(is_stderr);
        }
        _ => panic!("Expected Output update"),
    }
}

// ============= ProgressUpdate Completed Tests =============

#[tokio::test]
async fn test_progress_update_completed() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Completed {
        operation_id: "op_123".to_string(),
        message: "Build succeeded".to_string(),
        duration_ms: 5000,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::Completed {
            operation_id,
            message,
            duration_ms,
        } => {
            assert_eq!(operation_id, "op_123");
            assert_eq!(message, "Build succeeded");
            assert_eq!(*duration_ms, 5000);
        }
        _ => panic!("Expected Completed update"),
    }
}

// ============= ProgressUpdate Failed Tests =============

#[tokio::test]
async fn test_progress_update_failed() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Failed {
        operation_id: "op_123".to_string(),
        error: "Compilation error: missing semicolon".to_string(),
        duration_ms: 2000,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::Failed {
            operation_id,
            error,
            duration_ms,
        } => {
            assert_eq!(operation_id, "op_123");
            assert!(error.contains("Compilation error"));
            assert_eq!(*duration_ms, 2000);
        }
        _ => panic!("Expected Failed update"),
    }
}

// ============= ProgressUpdate Cancelled Tests =============

#[tokio::test]
async fn test_progress_update_cancelled() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::Cancelled {
        operation_id: "op_123".to_string(),
        message: "Operation cancelled by user".to_string(),
        duration_ms: 1000,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::Cancelled {
            operation_id,
            message,
            duration_ms,
        } => {
            assert_eq!(operation_id, "op_123");
            assert!(message.contains("cancelled"));
            assert_eq!(*duration_ms, 1000);
        }
        _ => panic!("Expected Cancelled update"),
    }
}

#[test]
fn test_format_cancellation_message_basic() {
    let message = format_cancellation_message("User requested cancellation", None, None);
    assert!(message.contains("User requested cancellation"));
}

#[test]
fn test_format_cancellation_message_with_tool() {
    let message =
        format_cancellation_message("Timeout exceeded", Some("cargo_build"), Some("op_123"));
    assert!(message.contains("cargo_build") || message.contains("Timeout exceeded"));
}

// ============= ProgressUpdate FinalResult Tests =============

#[tokio::test]
async fn test_progress_update_final_result_success() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::FinalResult {
        operation_id: "op_123".to_string(),
        command: "cargo build --release".to_string(),
        description: "Building in release mode".to_string(),
        working_directory: "/home/user/project".to_string(),
        success: true,
        full_output: "Finished release [optimized] target(s)".to_string(),
        duration_ms: 10000,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::FinalResult {
            operation_id,
            command,
            description,
            working_directory,
            success,
            full_output,
            duration_ms,
        } => {
            assert_eq!(operation_id, "op_123");
            assert_eq!(command, "cargo build --release");
            assert_eq!(description, "Building in release mode");
            assert_eq!(working_directory, "/home/user/project");
            assert!(success);
            assert!(full_output.contains("Finished"));
            assert_eq!(*duration_ms, 10000);
        }
        _ => panic!("Expected FinalResult update"),
    }
}

#[tokio::test]
async fn test_progress_update_final_result_failure() {
    let callback = MockCallbackSender::new();

    let update = ProgressUpdate::FinalResult {
        operation_id: "op_456".to_string(),
        command: "cargo test".to_string(),
        description: "Running tests".to_string(),
        working_directory: ".".to_string(),
        success: false,
        full_output: "test result: FAILED. 1 passed; 2 failed".to_string(),
        duration_ms: 5000,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    match &updates[0] {
        ProgressUpdate::FinalResult { success, .. } => {
            assert!(!success);
        }
        _ => panic!("Expected FinalResult update"),
    }
}

// ============= Client Type Filtering Tests =============

#[tokio::test]
async fn test_cursor_client_skips_progress() {
    let callback = MockCallbackSender::new().with_client_type(McpClientType::Cursor);

    let update = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Processing...".to_string(),
        percentage: Some(50.0),
        current_step: None,
    };

    // Should succeed but not record
    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    assert!(
        updates.is_empty(),
        "Cursor client should not receive progress updates"
    );
}

#[tokio::test]
async fn test_unknown_client_receives_progress() {
    let callback = MockCallbackSender::new().with_client_type(McpClientType::Unknown);

    let update = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Processing...".to_string(),
        percentage: Some(50.0),
        current_step: None,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    assert_eq!(updates.len(), 1);
}

#[tokio::test]
async fn test_vscode_client_receives_progress() {
    let callback = MockCallbackSender::new().with_client_type(McpClientType::VSCode);

    let update = ProgressUpdate::Progress {
        operation_id: "op_123".to_string(),
        message: "Processing...".to_string(),
        percentage: Some(50.0),
        current_step: None,
    };

    callback.send_progress(update).await.unwrap();

    let updates = callback.get_updates();
    assert_eq!(updates.len(), 1);
}

// ============= Error Handling Tests =============

#[tokio::test]
async fn test_callback_send_failure() {
    let callback = MockCallbackSender::new().with_failure();

    let update = ProgressUpdate::Started {
        operation_id: "op_123".to_string(),
        command: "echo test".to_string(),
        description: "Test".to_string(),
    };

    let result = callback.send_progress(update).await;
    assert!(result.is_err());

    match result {
        Err(CallbackError::SendFailed(msg)) => {
            assert!(msg.contains("Mock failure"));
        }
        _ => panic!("Expected SendFailed error"),
    }
}

// ============= Multiple Updates Sequence Test =============

#[tokio::test]
async fn test_full_operation_lifecycle() {
    let callback = MockCallbackSender::new();

    // Simulate full operation lifecycle
    let updates = vec![
        ProgressUpdate::Started {
            operation_id: "op_lifecycle".to_string(),
            command: "cargo build".to_string(),
            description: "Building".to_string(),
        },
        ProgressUpdate::Progress {
            operation_id: "op_lifecycle".to_string(),
            message: "Compiling dependencies".to_string(),
            percentage: Some(25.0),
            current_step: None,
        },
        ProgressUpdate::Output {
            operation_id: "op_lifecycle".to_string(),
            line: "Compiling serde v1.0".to_string(),
            is_stderr: false,
        },
        ProgressUpdate::Progress {
            operation_id: "op_lifecycle".to_string(),
            message: "Compiling main crate".to_string(),
            percentage: Some(75.0),
            current_step: None,
        },
        ProgressUpdate::Completed {
            operation_id: "op_lifecycle".to_string(),
            message: "Build complete".to_string(),
            duration_ms: 10000,
        },
    ];

    for update in updates {
        callback.send_progress(update).await.unwrap();
    }

    let received = callback.get_updates();
    assert_eq!(received.len(), 5);

    // Verify order
    assert!(matches!(&received[0], ProgressUpdate::Started { .. }));
    assert!(matches!(&received[1], ProgressUpdate::Progress { .. }));
    assert!(matches!(&received[2], ProgressUpdate::Output { .. }));
    assert!(matches!(&received[3], ProgressUpdate::Progress { .. }));
    assert!(matches!(&received[4], ProgressUpdate::Completed { .. }));
}

// ============= should_cancel Tests =============

#[tokio::test]
async fn test_should_cancel_returns_false() {
    let callback = MockCallbackSender::new();

    // Mock always returns false for should_cancel (matching McpCallbackSender behavior)
    assert!(!callback.should_cancel().await);
}
