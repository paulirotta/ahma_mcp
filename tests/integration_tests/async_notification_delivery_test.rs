use crate::common::test_utils::setup_test_environment_with_io;
use rmcp::{
    model::{CallToolRequestParam, Notification},
    server::{RequestContext, ServerHandler},
};
use serde_json::json;
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::sync::{mpsc::Receiver, Mutex};

async fn read_output(output_rx: &mut Receiver<String>) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(1), output_rx.recv())
        .await
        .ok()
        .flatten()
}

#[tokio::test]
async fn test_async_notification_delivery() {
    let (mcp_service, _mock_io, _input_tx, mut output_rx, _temp_dir) =
        setup_test_environment_with_io().await;
    let mcp_service = Arc::new(mcp_service);

    let notification_received = Arc::new(Mutex::new(false));
    let notification_received_clone = notification_received.clone();

    // Spawn a task to listen for the notification
    tokio::spawn(async move {
        while let Some(msg) = read_output(&mut output_rx).await {
            if let Ok(notification) = serde_json::from_str::<Notification>(&msg) {
                if notification.method == "display_text" {
                    let mut received_guard = notification_received_clone.lock().await;
                    *received_guard = true;
                    break;
                }
            }
        }
    });

    let mcp_service_clone = mcp_service.clone();
    let request_context = RequestContext::default();

    // 1. Start a long-running async tool
    let async_tool_params = json!({
        "duration": "2"
    });
    let call_params = CallToolRequestParam {
        name: Cow::Borrowed("long_running_async"),
        arguments: async_tool_params.as_object().cloned(),
    };
    tokio::spawn(async move {
        let _ = mcp_service_clone
            .call_tool(call_params, &request_context)
            .await;
    });

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 2. Call a fast, synchronous tool that blocks
    let sync_tool_params = json!({
        "command": "sleep 1" // Simulate a blocking operation
    });
    let sync_call_params = CallToolRequestParam {
        name: Cow::Borrowed("shell_sync"),
        arguments: sync_tool_params.as_object().cloned(),
    };
    let _ = mcp_service
        .call_tool(sync_call_params, &request_context)
        .await;

    // 3. Wait and check if the notification was received
    tokio::time::sleep(Duration::from_secs(3)).await;
    let received = notification_received.lock().await;
    assert!(
        *received,
        "Notification from the async tool should have been received even with a sync tool running."
    );
}
