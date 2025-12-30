//! Comprehensive unit tests for session.rs coverage gaps
//!
//! These tests focus on:
//! - URI parsing edge cases (percent encoding, localhost, query/fragment)
//! - Handshake timeout detection
//! - Session state queries under various conditions
//! - Error paths for message sending
//!
//! These tests improve session.rs coverage from ~47% to higher.

use ahma_http_bridge::session::{
    HandshakeState, McpRoot, SessionManager, SessionManagerConfig, SessionTerminationReason,
};
use std::path::PathBuf;
use tempfile::tempdir;

/// Helper to create a test session manager with echo as subprocess
fn create_test_session_manager(default_scope: PathBuf) -> SessionManager {
    let config = SessionManagerConfig {
        server_command: "echo".to_string(),
        server_args: vec!["test".to_string()],
        default_scope,
        enable_colored_output: false,
    };
    SessionManager::new(config)
}

// =============================================================================
// URI Parsing Tests - parse_file_uri_to_path via lock_sandbox
// =============================================================================

/// Test that file:// URIs with percent-encoded characters are decoded correctly
#[tokio::test]
async fn test_uri_percent_encoding_decoding() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // Path with spaces: /Users/test/My Project becomes file:///Users/test/My%20Project
    let roots = vec![McpRoot {
        uri: "file:///Users/test/My%20Project".to_string(),
        name: Some("My Project".to_string()),
    }];

    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should lock sandbox with percent-encoded URI");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    let scope = session
        .get_sandbox_scope()
        .await
        .expect("Should have scope");

    assert_eq!(
        scope,
        PathBuf::from("/Users/test/My Project"),
        "Percent-encoded spaces should be decoded"
    );
}

/// Test that file://localhost/ URIs are parsed correctly
#[tokio::test]
async fn test_uri_localhost_form() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // file://localhost/path/to/project is valid per RFC 8089
    let roots = vec![McpRoot {
        uri: "file://localhost/Users/dev/project".to_string(),
        name: None,
    }];

    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should lock sandbox with localhost URI");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    let scope = session
        .get_sandbox_scope()
        .await
        .expect("Should have scope");

    assert_eq!(
        scope,
        PathBuf::from("/Users/dev/project"),
        "file://localhost/ prefix should be stripped correctly"
    );
}

/// Test that query strings and fragments are stripped from URIs
#[tokio::test]
async fn test_uri_query_and_fragment_stripped() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // Some clients may add query or fragment to file URIs
    let roots = vec![McpRoot {
        uri: "file:///Users/dev/project?version=1#section".to_string(),
        name: None,
    }];

    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should lock sandbox with URI containing query/fragment");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    let scope = session
        .get_sandbox_scope()
        .await
        .expect("Should have scope");

    assert_eq!(
        scope,
        PathBuf::from("/Users/dev/project"),
        "Query string and fragment should be stripped"
    );
}

/// Test that non-file:// URIs are rejected (returns empty roots error)
#[tokio::test]
async fn test_non_file_uri_rejected() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // Only file:// URIs are valid for sandbox scope
    let roots = vec![McpRoot {
        uri: "https://example.com/project".to_string(),
        name: None,
    }];

    let result = session_manager.lock_sandbox(&session_id, &roots).await;

    assert!(
        result.is_err(),
        "Non-file:// URI should be rejected (no valid roots)"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("No valid sandbox roots"),
        "Error should mention no valid roots: {}",
        err_msg
    );
}

/// Test that relative paths (non-absolute) are rejected
#[tokio::test]
async fn test_relative_path_rejected() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // file:// must be followed by absolute path starting with /
    let roots = vec![McpRoot {
        uri: "file://relative/path".to_string(), // Missing leading /
        name: None,
    }];

    let result = session_manager.lock_sandbox(&session_id, &roots).await;

    assert!(result.is_err(), "Relative path URI should be rejected");
}

/// Test that malformed percent-encoding fails gracefully
#[tokio::test]
async fn test_malformed_percent_encoding_rejected() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // Incomplete percent encoding: %2 instead of %20
    let roots = vec![McpRoot {
        uri: "file:///path/with%2incomplete".to_string(),
        name: None,
    }];

    // This should fail because the percent decoding is malformed
    let result = session_manager.lock_sandbox(&session_id, &roots).await;

    // Malformed URI results in None from parse, leading to empty valid roots
    assert!(
        result.is_err(),
        "Malformed percent encoding should be rejected"
    );
}

/// Test multiple roots with mixed valid and invalid URIs
#[tokio::test]
async fn test_mixed_valid_invalid_roots() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // Mix of valid and invalid URIs - should accept the valid ones
    let roots = vec![
        McpRoot {
            uri: "https://invalid.com/path".to_string(), // Invalid - not file://
            name: None,
        },
        McpRoot {
            uri: "file:///valid/path/one".to_string(), // Valid
            name: Some("Valid One".to_string()),
        },
        McpRoot {
            uri: "file://no-slash".to_string(), // Invalid - no absolute path
            name: None,
        },
        McpRoot {
            uri: "file:///valid/path/two".to_string(), // Valid
            name: Some("Valid Two".to_string()),
        },
    ];

    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should accept roots with at least one valid URI");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    let scopes = session
        .get_sandbox_scopes()
        .await
        .expect("Should have scopes");

    // Should only include the valid file:// paths
    assert_eq!(scopes.len(), 2, "Should have exactly 2 valid scopes");
    assert!(scopes.contains(&PathBuf::from("/valid/path/one")));
    assert!(scopes.contains(&PathBuf::from("/valid/path/two")));
}

// =============================================================================
// Handshake Timeout Tests
// =============================================================================

// NOTE: Tests for handshake_timeout_secs() that manipulate environment variables
// are intentionally omitted. Env var tests are inherently flaky in parallel test
// execution because other tests may be reading/writing the same env var concurrently.
// The function is simple (env var parse with fallback) and is tested implicitly
// through integration tests that rely on timeout behavior.

/// Test is_handshake_timed_out returns None when sandbox is locked
#[tokio::test]
async fn test_handshake_timeout_returns_none_when_locked() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    // Lock the sandbox
    let roots = vec![McpRoot {
        uri: "file:///project".to_string(),
        name: None,
    }];
    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should lock sandbox");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    // Even if time has passed, locked sandbox means no timeout
    assert!(
        session.is_handshake_timed_out().is_none(),
        "Locked sandbox should not report timeout"
    );
}

/// Test is_handshake_timed_out returns None before timeout elapses
#[tokio::test]
async fn test_handshake_not_timed_out_initially() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    // Immediately after creation, should not be timed out
    // (default timeout is 30s, we're checking immediately)
    assert!(
        session.is_handshake_timed_out().is_none(),
        "New session should not be timed out"
    );
}

// =============================================================================
// Session State Query Tests
// =============================================================================

/// Test get_sandbox_scope returns first scope when multiple roots exist
#[tokio::test]
async fn test_get_sandbox_scope_returns_first() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager
        .create_session()
        .await
        .expect("Should create session");

    let roots = vec![
        McpRoot {
            uri: "file:///first/path".to_string(),
            name: None,
        },
        McpRoot {
            uri: "file:///second/path".to_string(),
            name: None,
        },
    ];

    session_manager
        .lock_sandbox(&session_id, &roots)
        .await
        .expect("Should lock sandbox");

    let session = session_manager
        .get_session(&session_id)
        .expect("Session should exist");

    let single_scope = session
        .get_sandbox_scope()
        .await
        .expect("Should have scope");

    let all_scopes = session
        .get_sandbox_scopes()
        .await
        .expect("Should have scopes");

    assert_eq!(
        single_scope,
        PathBuf::from("/first/path"),
        "get_sandbox_scope should return first scope"
    );
    assert_eq!(all_scopes.len(), 2, "get_sandbox_scopes should return all");
}

/// Test session_count reflects active sessions
#[tokio::test]
async fn test_session_count_tracking() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    assert_eq!(session_manager.session_count(), 0, "Initially no sessions");

    let session1 = session_manager.create_session().await.unwrap();
    assert_eq!(session_manager.session_count(), 1, "One session created");

    let session2 = session_manager.create_session().await.unwrap();
    assert_eq!(session_manager.session_count(), 2, "Two sessions created");

    session_manager
        .terminate_session(&session1, SessionTerminationReason::ClientRequested)
        .await
        .unwrap();
    assert_eq!(
        session_manager.session_count(),
        1,
        "One session after termination"
    );

    session_manager
        .terminate_session(&session2, SessionTerminationReason::ClientRequested)
        .await
        .unwrap();
    assert_eq!(
        session_manager.session_count(),
        0,
        "No sessions after all terminated"
    );
}

// =============================================================================
// Message Sending Error Path Tests
// =============================================================================

/// Test send_message fails for non-existent session
#[tokio::test]
async fn test_send_message_nonexistent_session() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let fake_session_id = "nonexistent-session-id";
    let message = serde_json::json!({"test": "message"});

    let result = session_manager
        .send_message(fake_session_id, &message)
        .await;

    assert!(result.is_err(), "Should fail for nonexistent session");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Session not found"),
        "Error should mention session not found"
    );
}

/// Test send_request fails for non-existent session
#[tokio::test]
async fn test_send_request_nonexistent_session() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let fake_session_id = "nonexistent-session-id";
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "test",
        "id": 1
    });

    let result = session_manager
        .send_request(fake_session_id, &request)
        .await;

    assert!(result.is_err(), "Should fail for nonexistent session");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Session not found"),
        "Error should mention session not found"
    );
}

/// Test send_message fails for terminated session
#[tokio::test]
async fn test_send_message_terminated_session() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager.create_session().await.unwrap();

    // Terminate the session
    session_manager
        .terminate_session(&session_id, SessionTerminationReason::ClientRequested)
        .await
        .unwrap();

    // Try to send message to terminated session - should fail
    // Note: After termination, session is removed from map, so this becomes "not found"
    let message = serde_json::json!({"test": "message"});
    let result = session_manager.send_message(&session_id, &message).await;

    assert!(result.is_err(), "Should fail for terminated session");
}

/// Test handle_roots_changed before sandbox lock is allowed
#[tokio::test]
async fn test_roots_changed_before_lock_allowed() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager.create_session().await.unwrap();

    // Call handle_roots_changed BEFORE locking sandbox
    let result = session_manager.handle_roots_changed(&session_id).await;

    assert!(
        result.is_ok(),
        "Roots change before sandbox lock should be allowed (with warning)"
    );

    // Session should still exist
    assert!(
        session_manager.session_exists(&session_id),
        "Session should still exist after pre-lock roots change"
    );
}

// =============================================================================
// Termination Reason Coverage
// =============================================================================

/// Test all termination reasons can be used
#[tokio::test]
async fn test_termination_reasons() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    // Test each termination reason
    let reasons = [
        SessionTerminationReason::ClientRequested,
        SessionTerminationReason::RootsChangeRejected,
        SessionTerminationReason::ProcessCrashed,
        SessionTerminationReason::Timeout,
    ];

    for reason in reasons {
        let session_id = session_manager.create_session().await.unwrap();

        let result = session_manager.terminate_session(&session_id, reason).await;

        assert!(
            result.is_ok(),
            "Termination with reason {:?} should succeed",
            reason
        );
        assert!(
            !session_manager.session_exists(&session_id),
            "Session should be removed after {:?} termination",
            reason
        );
    }
}

/// Test terminating non-existent session is a no-op
#[tokio::test]
async fn test_terminate_nonexistent_session_noop() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let result = session_manager
        .terminate_session("nonexistent", SessionTerminationReason::ClientRequested)
        .await;

    assert!(
        result.is_ok(),
        "Terminating nonexistent session should be ok (no-op)"
    );
}

// =============================================================================
// HandshakeState Additional Coverage
// =============================================================================

/// Test handshake state queries on session object
#[tokio::test]
async fn test_session_handshake_state_queries() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager.create_session().await.unwrap();
    let session = session_manager.get_session(&session_id).unwrap();

    // Initial state
    assert_eq!(
        session.handshake_state(),
        HandshakeState::AwaitingBoth,
        "New session should be in AwaitingBoth state"
    );

    assert!(
        !session.is_terminated(),
        "New session should not be terminated"
    );

    assert!(
        !session.is_sandbox_locked(),
        "New session should not have sandbox locked"
    );
}

/// Test subscribe returns a broadcast receiver
#[tokio::test]
async fn test_session_subscribe() {
    let temp = tempdir().expect("Failed to create temp dir");
    let session_manager = create_test_session_manager(temp.path().to_path_buf());

    let session_id = session_manager.create_session().await.unwrap();
    let session = session_manager.get_session(&session_id).unwrap();

    // Should be able to subscribe to SSE events
    let _receiver = session.subscribe();

    // Getting a second receiver should also work
    let _receiver2 = session.subscribe();
}
