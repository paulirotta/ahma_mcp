
// tests/async_notification_delivery_test.rs

use super::*;
use crate::mcp_service::McpService;
use crate::test_utils::setup_test_environment;
use rmcp::Notification;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

#[tokio::test]
async fn test_async_notification_delivery_while_blocked() {
    // 1. Setup the test environment
    let (mcp_service, mock_io, _temp_dir) = setup_test_environment().await;
    let mcp_service = Arc::new(mcp_service);

    // Flag to confirm the async notification was received
    let notification_received = Arc::new(AtomicBool::new(false));
    let notification_received_clone = notification_received.clone();

    // 2. Start a long-running asynchronous tool
    // We'll use a simple shell command that sleeps and then echoes a message.
    // The key is that this runs in the background.
    let async_tool_params = json!({
        "command": "sleep 2 && echo 'Async task complete'"
    });

    // We need a way to capture the notification. We'll spawn a task to listen for it.
    let mock_io_clone = mock_io.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(Some(msg)) = mock_io_clone.lock().await.recv().await {
                if let Ok(notification) = serde_json::from_str::<Notification>(&msg) {
                    if notification.method == "mcp/operationCompleted" {
                        // A real implementation would check the operation ID and result.
                        // For this test, just seeing the completion is enough.
                        notification_received_clone.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
            sleep(Duration::from_millis(100)).await;
        }
    });

    let mcp_service_clone = mcp_service.clone();
    tokio::spawn(async move {
        mcp_service_clone.call_tool("shell", async_tool_params).await;
    });

    // Give the async tool a moment to start
    sleep(Duration::from_millis(500)).await;

    // 3. Immediately call a synchronous, blocking tool
    // This will block the main flow, simulating the agent being busy.
    // We'll use a simple synchronous sleep command.
    let sync_tool_params = json!({
        "command": "sleep 3"
    });
    mcp_service.call_tool("shell_sync", sync_tool_params).await;

    // 4. Verify the notification was received
    // The synchronous tool slept for 3 seconds. The async tool finished after 2 seconds.
    // By the time the synchronous tool unblocks, the notification should have been sent
    // and received by our listener task.
    assert!(
        notification_received.load(Ordering::SeqCst),
        "Asynchronous notification was dropped while the client was blocked."
    );
}
