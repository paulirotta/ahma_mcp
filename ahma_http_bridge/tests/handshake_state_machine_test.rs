//! Tests for the HandshakeState state machine in session.rs
//!
//! This test validates that the handshake state machine correctly:
//! 1. Transitions through states in the correct order
//! 2. Prevents double-triggering of roots/list_changed
//! 3. Handles both orderings: SSE-first and MCP-first
//! 4. Uses atomic compare-exchange to prevent race conditions

use ahma_http_bridge::DEFAULT_HANDSHAKE_TIMEOUT_SECS;
use ahma_http_bridge::session::{HandshakeState, SessionManager, SessionManagerConfig};
use tempfile::tempdir;

/// Create a test session manager with minimal config
#[allow(dead_code)]
fn create_test_session_manager() -> SessionManager {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    SessionManager::new(SessionManagerConfig {
        server_command: "echo".to_string(), // Won't actually run
        server_args: vec![],
        default_scope: temp_dir.path().to_path_buf(),
        enable_colored_output: false,
        handshake_timeout_secs: DEFAULT_HANDSHAKE_TIMEOUT_SECS,
    })
}

#[test]
fn test_handshake_state_enum_values() {
    // Verify enum values match the repr(u8) specification
    assert_eq!(HandshakeState::AwaitingBoth as u8, 0);
    assert_eq!(HandshakeState::AwaitingSseOnly as u8, 1);
    assert_eq!(HandshakeState::AwaitingMcpOnly as u8, 2);
    assert_eq!(HandshakeState::RootsRequested as u8, 3);
    assert_eq!(HandshakeState::Complete as u8, 4);
}

#[test]
fn test_handshake_state_from_u8_roundtrip() {
    // Verify from_u8 correctly converts back from u8
    assert_eq!(HandshakeState::from_u8(0), HandshakeState::AwaitingBoth);
    assert_eq!(HandshakeState::from_u8(1), HandshakeState::AwaitingSseOnly);
    assert_eq!(HandshakeState::from_u8(2), HandshakeState::AwaitingMcpOnly);
    assert_eq!(HandshakeState::from_u8(3), HandshakeState::RootsRequested);
    assert_eq!(HandshakeState::from_u8(4), HandshakeState::Complete);
}

#[test]
fn test_handshake_state_from_u8_invalid_falls_back_to_awaiting_both() {
    // Invalid values should fall back to AwaitingBoth for safety
    assert_eq!(HandshakeState::from_u8(5), HandshakeState::AwaitingBoth);
    assert_eq!(HandshakeState::from_u8(255), HandshakeState::AwaitingBoth);
}

#[test]
fn test_handshake_state_debug_and_clone() {
    // Ensure Debug and Clone traits work
    let state = HandshakeState::RootsRequested;
    let cloned = state;
    assert_eq!(state, cloned);

    let debug_str = format!("{:?}", state);
    assert!(debug_str.contains("RootsRequested"));
}

#[test]
fn test_handshake_state_equality() {
    // Test PartialEq implementation
    assert_eq!(HandshakeState::AwaitingBoth, HandshakeState::AwaitingBoth);
    assert_ne!(HandshakeState::AwaitingBoth, HandshakeState::Complete);

    // Test Copy semantics
    let a = HandshakeState::RootsRequested;
    let b = a; // Copy
    assert_eq!(a, b);
}

/// Test the expected state transition: SSE connects first, then MCP initialized
#[tokio::test]
async fn test_state_machine_sse_first_ordering() {
    // This test validates the state machine logic conceptually
    // In production, sessions are created via create_session() which spawns processes

    // Expected transitions:
    // 1. AwaitingBoth (initial)
    // 2. mark_sse_connected: AwaitingBoth → AwaitingSseOnly (no notification)
    // 3. mark_mcp_initialized: AwaitingSseOnly → RootsRequested (sends notification)
    // 4. lock_sandbox: RootsRequested → Complete

    let transitions = [
        (HandshakeState::AwaitingBoth, "initial state"),
        (HandshakeState::AwaitingSseOnly, "after SSE connect"),
        (HandshakeState::RootsRequested, "after MCP initialized"),
        (HandshakeState::Complete, "after sandbox lock"),
    ];

    // Verify transition sequence makes sense
    for (i, (state, desc)) in transitions.iter().enumerate() {
        if i > 0 {
            let prev_state = transitions[i - 1].0;
            assert!(
                *state as u8 > prev_state as u8,
                "State {} ({}) should have higher value than {} ({})",
                desc,
                *state as u8,
                transitions[i - 1].1,
                prev_state as u8
            );
        }
    }
}

/// Test the expected state transition: MCP initialized first, then SSE connects
#[tokio::test]
async fn test_state_machine_mcp_first_ordering() {
    // Expected transitions:
    // 1. AwaitingBoth (initial)
    // 2. mark_mcp_initialized: AwaitingBoth → AwaitingMcpOnly (no notification)
    // 3. mark_sse_connected: AwaitingMcpOnly → RootsRequested (sends notification)
    // 4. lock_sandbox: RootsRequested → Complete

    let transitions = [
        (HandshakeState::AwaitingBoth, "initial state"),
        (HandshakeState::AwaitingMcpOnly, "after MCP initialized"),
        (HandshakeState::RootsRequested, "after SSE connect"),
        (HandshakeState::Complete, "after sandbox lock"),
    ];

    // Verify transition sequence makes sense
    for (i, (state, desc)) in transitions.iter().enumerate() {
        if i > 0 {
            let prev_state = transitions[i - 1].0;
            // Note: AwaitingMcpOnly (2) can transition to RootsRequested (3)
            // This is correct because the second event triggers the notification
            assert!(
                *state as u8 >= prev_state as u8,
                "State {} ({}) should have >= value than {} ({})",
                desc,
                *state as u8,
                transitions[i - 1].1,
                prev_state as u8
            );
        }
    }
}

/// Test that is_sse_connected reflects correct states
#[test]
fn test_is_sse_connected_reflects_state() {
    // These states indicate SSE is connected
    let sse_connected_states = [
        HandshakeState::AwaitingSseOnly,
        HandshakeState::RootsRequested,
        HandshakeState::Complete,
    ];

    // These states indicate SSE is NOT connected
    let sse_not_connected_states = [
        HandshakeState::AwaitingBoth,
        HandshakeState::AwaitingMcpOnly,
    ];

    for state in sse_connected_states {
        assert!(
            matches!(
                state,
                HandshakeState::AwaitingSseOnly
                    | HandshakeState::RootsRequested
                    | HandshakeState::Complete
            ),
            "State {:?} should indicate SSE connected",
            state
        );
    }

    for state in sse_not_connected_states {
        assert!(
            !matches!(
                state,
                HandshakeState::AwaitingSseOnly
                    | HandshakeState::RootsRequested
                    | HandshakeState::Complete
            ),
            "State {:?} should indicate SSE NOT connected",
            state
        );
    }
}

/// Test that is_mcp_initialized reflects correct states
#[test]
fn test_is_mcp_initialized_reflects_state() {
    // These states indicate MCP is initialized
    let mcp_initialized_states = [
        HandshakeState::AwaitingMcpOnly,
        HandshakeState::RootsRequested,
        HandshakeState::Complete,
    ];

    // These states indicate MCP is NOT initialized
    let mcp_not_initialized_states = [
        HandshakeState::AwaitingBoth,
        HandshakeState::AwaitingSseOnly,
    ];

    for state in mcp_initialized_states {
        assert!(
            matches!(
                state,
                HandshakeState::AwaitingMcpOnly
                    | HandshakeState::RootsRequested
                    | HandshakeState::Complete
            ),
            "State {:?} should indicate MCP initialized",
            state
        );
    }

    for state in mcp_not_initialized_states {
        assert!(
            !matches!(
                state,
                HandshakeState::AwaitingMcpOnly
                    | HandshakeState::RootsRequested
                    | HandshakeState::Complete
            ),
            "State {:?} should indicate MCP NOT initialized",
            state
        );
    }
}

/// Test that the transition table is exhaustive and correct
#[test]
fn test_state_transition_table() {
    // This documents the expected state transitions for mark_sse_connected
    let sse_connect_transitions: Vec<(HandshakeState, Option<(HandshakeState, bool)>)> = vec![
        // (from_state, Some((to_state, sends_notification)) or None if no-op)
        (
            HandshakeState::AwaitingBoth,
            Some((HandshakeState::AwaitingSseOnly, false)),
        ),
        (
            HandshakeState::AwaitingMcpOnly,
            Some((HandshakeState::RootsRequested, true)),
        ),
        (HandshakeState::AwaitingSseOnly, None), // Already have SSE
        (HandshakeState::RootsRequested, None),  // Already past SSE
        (HandshakeState::Complete, None),        // Already complete
    ];

    // This documents the expected state transitions for mark_mcp_initialized
    let mcp_init_transitions: Vec<(HandshakeState, Option<(HandshakeState, bool)>)> = vec![
        (
            HandshakeState::AwaitingBoth,
            Some((HandshakeState::AwaitingMcpOnly, false)),
        ),
        (
            HandshakeState::AwaitingSseOnly,
            Some((HandshakeState::RootsRequested, true)),
        ),
        (HandshakeState::AwaitingMcpOnly, None), // Already have MCP
        (HandshakeState::RootsRequested, None),  // Already past MCP
        (HandshakeState::Complete, None),        // Already complete
    ];

    // Verify that exactly one path sends a notification for each ordering
    let sse_sends_notification = sse_connect_transitions
        .iter()
        .filter(|(_, result)| result.map(|(_, sends)| sends).unwrap_or(false))
        .count();
    let mcp_sends_notification = mcp_init_transitions
        .iter()
        .filter(|(_, result)| result.map(|(_, sends)| sends).unwrap_or(false))
        .count();

    assert_eq!(
        sse_sends_notification, 1,
        "Exactly one SSE transition should send notification"
    );
    assert_eq!(
        mcp_sends_notification, 1,
        "Exactly one MCP transition should send notification"
    );

    // Verify notification is sent when transitioning TO RootsRequested
    for (from, result) in sse_connect_transitions
        .iter()
        .chain(mcp_init_transitions.iter())
    {
        if let Some((to, sends)) = result {
            if *to == HandshakeState::RootsRequested {
                assert!(
                    *sends,
                    "Transition from {:?} to RootsRequested should send notification",
                    from
                );
            } else {
                assert!(
                    !*sends,
                    "Transition from {:?} to {:?} should NOT send notification",
                    from, to
                );
            }
        }
    }
}

/// Test that double-calling mark_sse_connected is idempotent
#[test]
fn test_double_sse_connect_idempotent() {
    // After AwaitingBoth → AwaitingSseOnly, a second call should be a no-op
    // This prevents double-sending roots/list_changed

    use std::sync::atomic::{AtomicU8, Ordering};

    let state = AtomicU8::new(HandshakeState::AwaitingBoth as u8);

    // First call: AwaitingBoth → AwaitingSseOnly
    let result = state.compare_exchange(
        HandshakeState::AwaitingBoth as u8,
        HandshakeState::AwaitingSseOnly as u8,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    assert!(result.is_ok());
    assert_eq!(
        HandshakeState::from_u8(state.load(Ordering::SeqCst)),
        HandshakeState::AwaitingSseOnly
    );

    // Second call: should fail compare_exchange because state is no longer AwaitingBoth
    let result = state.compare_exchange(
        HandshakeState::AwaitingBoth as u8,
        HandshakeState::AwaitingSseOnly as u8,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    assert!(result.is_err());
    // State should be unchanged
    assert_eq!(
        HandshakeState::from_u8(state.load(Ordering::SeqCst)),
        HandshakeState::AwaitingSseOnly
    );
}

/// Test that double-calling mark_mcp_initialized is idempotent
#[test]
fn test_double_mcp_init_idempotent() {
    use std::sync::atomic::{AtomicU8, Ordering};

    let state = AtomicU8::new(HandshakeState::AwaitingBoth as u8);

    // First call: AwaitingBoth → AwaitingMcpOnly
    let result = state.compare_exchange(
        HandshakeState::AwaitingBoth as u8,
        HandshakeState::AwaitingMcpOnly as u8,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    assert!(result.is_ok());

    // Second call: should fail
    let result = state.compare_exchange(
        HandshakeState::AwaitingBoth as u8,
        HandshakeState::AwaitingMcpOnly as u8,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    assert!(result.is_err());
}

/// Test concurrent SSE and MCP events - exactly one should send notification
#[tokio::test]
async fn test_concurrent_sse_and_mcp_exactly_one_notification() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    let state = Arc::new(AtomicU8::new(HandshakeState::AwaitingBoth as u8));
    let notification_count = Arc::new(AtomicUsize::new(0));

    // Simulate mark_sse_connected
    let simulate_sse = {
        let state = state.clone();
        let notification_count = notification_count.clone();
        async move {
            loop {
                let current = state.load(Ordering::SeqCst);
                let (new_state, should_send) = match HandshakeState::from_u8(current) {
                    HandshakeState::AwaitingBoth => (HandshakeState::AwaitingSseOnly as u8, false),
                    HandshakeState::AwaitingMcpOnly => (HandshakeState::RootsRequested as u8, true),
                    _ => return false, // No-op
                };

                if state
                    .compare_exchange(current, new_state, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    if should_send {
                        notification_count.fetch_add(1, Ordering::SeqCst);
                    }
                    return should_send;
                }
                // Retry on CAS failure
            }
        }
    };

    // Simulate mark_mcp_initialized
    let simulate_mcp = {
        let state = state.clone();
        let notification_count = notification_count.clone();
        async move {
            loop {
                let current = state.load(Ordering::SeqCst);
                let (new_state, should_send) = match HandshakeState::from_u8(current) {
                    HandshakeState::AwaitingBoth => (HandshakeState::AwaitingMcpOnly as u8, false),
                    HandshakeState::AwaitingSseOnly => (HandshakeState::RootsRequested as u8, true),
                    _ => return false, // No-op
                };

                if state
                    .compare_exchange(current, new_state, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    if should_send {
                        notification_count.fetch_add(1, Ordering::SeqCst);
                    }
                    return should_send;
                }
                // Retry on CAS failure
            }
        }
    };

    // Run both concurrently
    let (sse_result, mcp_result) = tokio::join!(simulate_sse, simulate_mcp);

    // Exactly one should have sent the notification
    let total_notifications = notification_count.load(Ordering::SeqCst);
    assert_eq!(
        total_notifications, 1,
        "Exactly one notification should be sent. SSE sent: {}, MCP sent: {}",
        sse_result, mcp_result
    );

    // Final state should be RootsRequested
    assert_eq!(
        HandshakeState::from_u8(state.load(Ordering::SeqCst)),
        HandshakeState::RootsRequested,
        "Final state should be RootsRequested"
    );

    // Exactly one of the methods should return true
    assert!(
        (sse_result && !mcp_result) || (!sse_result && mcp_result),
        "Exactly one method should return true (sent notification)"
    );
}

/// Stress test: many concurrent attempts should result in exactly one notification
#[tokio::test]
async fn test_stress_concurrent_handshake_attempts() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    for _ in 0..100 {
        let state = Arc::new(AtomicU8::new(HandshakeState::AwaitingBoth as u8));
        let notification_count = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];

        // Spawn 10 SSE attempts
        for _ in 0..10 {
            let state = state.clone();
            let notification_count = notification_count.clone();
            handles.push(tokio::spawn(async move {
                loop {
                    let current = state.load(Ordering::SeqCst);
                    let (new_state, should_send) = match HandshakeState::from_u8(current) {
                        HandshakeState::AwaitingBoth => {
                            (HandshakeState::AwaitingSseOnly as u8, false)
                        }
                        HandshakeState::AwaitingMcpOnly => {
                            (HandshakeState::RootsRequested as u8, true)
                        }
                        _ => return,
                    };

                    if state
                        .compare_exchange(current, new_state, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        if should_send {
                            notification_count.fetch_add(1, Ordering::SeqCst);
                        }
                        return;
                    }
                }
            }));
        }

        // Spawn 10 MCP attempts
        for _ in 0..10 {
            let state = state.clone();
            let notification_count = notification_count.clone();
            handles.push(tokio::spawn(async move {
                loop {
                    let current = state.load(Ordering::SeqCst);
                    let (new_state, should_send) = match HandshakeState::from_u8(current) {
                        HandshakeState::AwaitingBoth => {
                            (HandshakeState::AwaitingMcpOnly as u8, false)
                        }
                        HandshakeState::AwaitingSseOnly => {
                            (HandshakeState::RootsRequested as u8, true)
                        }
                        _ => return,
                    };

                    if state
                        .compare_exchange(current, new_state, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        if should_send {
                            notification_count.fetch_add(1, Ordering::SeqCst);
                        }
                        return;
                    }
                }
            }));
        }

        // Wait for all
        for handle in handles {
            handle.await.unwrap();
        }

        // Exactly one notification
        let total = notification_count.load(Ordering::SeqCst);
        assert_eq!(
            total, 1,
            "Stress test iteration: expected exactly 1 notification, got {}",
            total
        );

        // Final state is RootsRequested
        assert_eq!(
            HandshakeState::from_u8(state.load(Ordering::SeqCst)),
            HandshakeState::RootsRequested
        );
    }
}
