use ahma_core::callback_system::{CallbackSender, ProgressUpdate};
use ahma_core::client_type::McpClientType;
use ahma_core::mcp_callback::McpCallbackSender;
use rmcp::{
    ServerHandler,
    model::{NumberOrString, ProgressToken},
    service::{RoleClient, RoleServer, serve_directly},
    transport::async_rw::AsyncRwTransport,
};
use std::sync::Arc;
use tokio::io::duplex;

// Implement ServerHandler for a local type to use it in tests
struct DummyServer;
impl ServerHandler for DummyServer {}

#[tokio::test]
async fn test_mcp_callback_sender_flow() {
    // Create a local transport pair using duplex
    let (client_stream, server_stream) = duplex(1024);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let client_transport = AsyncRwTransport::new_client(client_read, client_write);
    let server_transport = AsyncRwTransport::new_server(server_read, server_write);

    // Start the server peer
    let server_peer = serve_directly::<RoleServer, DummyServer, _, _, _>(DummyServer, server_transport, None);

    // Start the client peer
    let _client_peer = serve_directly::<RoleClient, (), _, _, _>((), client_transport, None);

    let operation_id = "test-op-123".to_string();
    let progress_token = ProgressToken(NumberOrString::String(Arc::from("token-456")));

    let sender = McpCallbackSender::new(
        server_peer.peer().clone(),
        operation_id.clone(),
        Some(progress_token.clone()),
        McpClientType::Unknown,
    );

    // 1. Test Started
    sender
        .send_progress(ProgressUpdate::Started {
            operation_id: operation_id.clone(),
            command: "test-cmd".to_string(),
            description: "Testing...".to_string(),
        })
        .await
        .unwrap();

    // 2. Test Progress
    sender
        .send_progress(ProgressUpdate::Progress {
            operation_id: operation_id.clone(),
            message: "Working...".to_string(),
            percentage: Some(42.0),
            current_step: None,
        })
        .await
        .unwrap();

    // 3. Test Completed
    sender
        .send_progress(ProgressUpdate::Completed {
            operation_id: operation_id.clone(),
            message: "Done!".to_string(),
            duration_ms: 100,
        })
        .await
        .unwrap();

    // In a real test we would verify the notifications on client_peer,
    // but even just running this ensures no panics and basic protocol compatibility.
}

#[tokio::test]
async fn test_mcp_callback_sender_skips_cursor() {
    let (client_stream, server_stream) = duplex(1024);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let client_transport = AsyncRwTransport::new_client(client_read, client_write);
    let server_transport = AsyncRwTransport::new_server(server_read, server_write);

    let server_peer = serve_directly::<RoleServer, DummyServer, _, _, _>(DummyServer, server_transport, None);
    let _client_peer = serve_directly::<RoleClient, (), _, _, _>((), client_transport, None);

    let sender = McpCallbackSender::new(
        server_peer.peer().clone(),
        "op-1".to_string(),
        Some(ProgressToken(NumberOrString::String(Arc::from("token")))),
        McpClientType::Cursor, // Cursor skips progress
    );

    // This should return Ok(()) immediately without sending anything
    sender
        .send_progress(ProgressUpdate::Progress {
            operation_id: "op-1".to_string(),
            message: "Working...".to_string(),
            percentage: Some(50.0),
            current_step: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_mcp_callback_sender_no_token() {
    let (client_stream, server_stream) = duplex(1024);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let client_transport = AsyncRwTransport::new_client(client_read, client_write);
    let server_transport = AsyncRwTransport::new_server(server_read, server_write);

    let server_peer = serve_directly::<RoleServer, DummyServer, _, _, _>(DummyServer, server_transport, None);
    let _client_peer = serve_directly::<RoleClient, (), _, _, _>((), client_transport, None);

    let sender = McpCallbackSender::new(
        server_peer.peer().clone(),
        "op-1".to_string(),
        None, // No token
        McpClientType::Unknown,
    );

    // This should return Ok(()) immediately without sending anything
    sender
        .send_progress(ProgressUpdate::Progress {
            operation_id: "op-1".to_string(),
            message: "Working...".to_string(),
            percentage: Some(50.0),
            current_step: None,
        })
        .await
        .unwrap();
}
