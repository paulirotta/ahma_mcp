use ahma_http_bridge::session::{
    HandshakeState, SessionManager, SessionManagerConfig, SessionTerminationReason,
};

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_handshake_state_machine_transitions() {
    // Setup session manager with mock components
    let config = SessionManagerConfig {
        server_command: "true".to_string(), // No-op command
        server_args: vec![],
        default_scope: Some(std::path::PathBuf::from(".")),
        enable_colored_output: false,
        handshake_timeout_secs: 5,
    };
    let session_manager = SessionManager::new(config);
    let session_id = session_manager
        .create_session()
        .await
        .expect("Failed to create session");
    let session = session_manager
        .get_session(&session_id)
        .expect("Session not found");

    // Initial state
    assert_eq!(session.handshake_state(), HandshakeState::AwaitingBoth);

    // Transition 1: SSE Connected
    let sent = session
        .mark_sse_connected()
        .await
        .expect("SSE connect failed");
    assert!(!sent, "Should not send roots/list_changed yet");
    assert_eq!(session.handshake_state(), HandshakeState::AwaitingSseOnly);

    // Transition 2: MCP Initialized
    let sent = session
        .mark_mcp_initialized()
        .await
        .expect("MCP init failed");
    assert!(sent, "Should send roots/list_changed now");
    assert_eq!(session.handshake_state(), HandshakeState::RootsRequested);

    // Cleanup
    session_manager
        .terminate_session(&session_id, SessionTerminationReason::ClientRequested)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_handshake_state_machine_race_condition_stress() {
    let config = SessionManagerConfig {
        server_command: "sh".to_string(), // Use sh to ignore extra args
        server_args: vec!["-c".to_string(), "sleep 10".to_string()],
        default_scope: Some(std::path::PathBuf::from(".")),
        enable_colored_output: false,
        handshake_timeout_secs: 5,
    };
    let session_manager = Arc::new(SessionManager::new(config));

    // Run many concurrent iterations to catch races
    for i in 0..100 {
        let session_id = session_manager
            .create_session()
            .await
            .expect("Failed to create session");
        let session = session_manager
            .get_session(&session_id)
            .expect("Session not found");

        let session_clone1 = session.clone();
        let session_clone2 = session.clone();

        let t1 = tokio::spawn(async move {
            sleep(Duration::from_millis(1)).await; // Tiny unexpected delay
            session_clone1.mark_sse_connected().await
        });

        let t2 = tokio::spawn(async move { session_clone2.mark_mcp_initialized().await });

        let (r1, r2) = tokio::join!(t1, t2);
        let sent1 = r1.unwrap().unwrap();
        let sent2 = r2.unwrap().unwrap();

        // EXACTLY ONE of them should return true (triggered the notification)
        assert!(
            sent1 ^ sent2,
            "Exactly one path should trigger roots/list_changed. Iter: {}, s1: {}, s2: {}",
            i,
            sent1,
            sent2
        );

        assert_eq!(session.handshake_state(), HandshakeState::RootsRequested);

        session_manager
            .terminate_session(&session_id, SessionTerminationReason::ClientRequested)
            .await
            .unwrap();
    }
}
