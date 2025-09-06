use super::common::test_utils::{MockIo, setup_test_environment};
use ahma_mcp::mcp_service::AhmaMcpService;
use rmcp::{
    model::{CallToolRequestParam, Notification},
    server::{RequestContext, ServerHandler},
};
use serde_json::json;
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[tokio::test]
async fn test_async_notification_delivery() {
    let (mcp_service, mock_io, _temp_dir) = setup_test_environment().await;
    let mcp_service = Arc::new(mcp_service);
    let mock_io = Arc::new(Mutex::new(mock_io));

    let notification_received = Arc::new(Mutex::new(false));
    let notification_received_clone = notification_received.clone();
    let mock_io_clone = mock_io.clone();

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let mut mock_io_guard = mock_io_clone.lock().await;
            if let Ok(Some(msg)) = mock_io_guard.recv_string().await {
                if let Ok(notification) = serde_json::from_str::<Notification>(&msg) {
                    if notification.method == "display_text" {
                        let mut received = notification_received_clone.lock().await;
                        *received = true;
                        break;
                    }
                }
            }
        }
    });

    let mcp_service_clone = mcp_service.clone();
    let request_context = RequestContext::default();

    // 1. Start a long-running async tool
    let async_tool_params = json!({
        "command": "sleep 2 && echo 'async task done'"
    });
    let call_params = CallToolRequestParam {
        name: Cow::Borrowed("shell"),
        arguments: async_tool_params.as_object().cloned(),
    };
    tokio::spawn(async move {
        let _ = mcp_service_clone
            .call_tool(call_params, &request_context)
            .await;
    });

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 2. Call a fast, synchronous tool
    let sync_tool_params = json!({
        "command": "echo 'sync task'"
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
