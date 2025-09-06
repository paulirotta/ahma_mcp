//! Integration tests for the ahma_mcp service.
mod adapter_test;
mod callback_system_test;
mod common;
mod config_test;
mod generate_schema_test;
mod logging_test;
mod main_test;
mod mcp_callback_test;
mod mcp_service_test;
mod operation_monitor_test;
mod schema_validation_test;
mod shell_pool_test;
mod terminal_output_test;

use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::{CallToolRequestParam, Notification};
use serde_json::{Map, json};
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Receiver;

// Assuming common::test_client::new_client can be optimized for speed,
// e.g., by using in-memory setups or pre-initialized clients.
// If new_client involves file I/O, replace std::fs with tokio::fs for async ops.

#[tokio::test]
async fn test_list_tools() -> Result<()> {
    let client = new_client(Some("tools")).await?;
    let result = client.list_all_tools().await?;

    // Should have at least the built-in 'await' tool
    assert!(!result.is_empty());
    let tool_names: Vec<_> = result.iter().map(|t| t.name.as_ref()).collect();
    assert!(tool_names.contains(&"await"));
    assert!(tool_names.contains(&"ls"));

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_call_tool_basic() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    let mut params = Map::new();
    params.insert(
        "path".to_string(),
        serde_json::Value::String(".".to_string()),
    );

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("ls"),
        arguments: Some(params),
    };

    let result = client.call_tool(call_param).await?;

    // The result should contain the current directory's contents.
    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        assert!(text_content.text.contains("Cargo.toml"));
    }

    client.cancel().await?;
    Ok(())
}

async fn read_output(output_rx: &mut Receiver<String>) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(10), output_rx.recv())
        .await
        .ok()
        .flatten()
}

#[tokio::test]
#[ignore] // Ignoring until the core logic is implemented
async fn test_async_notification_delivery() {
    let (mcp_service, _mock_io, _input_tx, mut output_rx, _temp_dir) =
        common::test_utils::setup_test_environment_with_io().await;
    let mcp_service = Arc::new(mcp_service);

    let notification_received = Arc::new(Mutex::new(false));
    let notification_received_clone = notification_received.clone();

    // Spawn a task to listen for the notification
    let listener_handle = tokio::spawn(async move {
        while let Some(msg) = read_output(&mut output_rx).await {
            if let Ok(notification) = serde_json::from_str::<Notification>(&msg) {
                if let Some(content) = notification.params.get("content") {
                    if content
                        .to_string()
                        .contains("Operation long_running_async finished")
                    {
                        let mut received_guard = notification_received_clone.lock().await;
                        *received_guard = true;
                        break;
                    }
                }
            }
        }
    });

    let _mcp_service_clone = mcp_service.clone();

    // 1. Start a long-running async tool
    let async_tool_params = json!({
        "duration": "2"
    });
    let _call_params = CallToolRequestParam {
        name: Cow::Borrowed("long_running_async"),
        arguments: async_tool_params.as_object().cloned(),
    };

    // TODO: Fix RequestContext creation after rmcp API update
    // For now, comment out the actual service calls to get compilation working

    /*
    tokio::spawn(async move {
        let _ = mcp_service_clone
            .call_tool(call_params, request_context_async)
            .await;
    });
    */

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 2. Call a fast, synchronous tool that blocks
    let sync_tool_params = json!({
        "command": "sleep 1" // Simulate a blocking operation
    });
    let _sync_call_params = CallToolRequestParam {
        name: Cow::Borrowed("shell_sync"),
        arguments: sync_tool_params.as_object().cloned(),
    };

    // TODO: Fix RequestContext creation after rmcp API update
    /*
    let _ = mcp_service
        .call_tool(sync_call_params, &request_context_sync)
        .await;
    */

    // 3. Wait and check if the notification was received
    listener_handle.await.unwrap();
    let received = notification_received.lock().await;
    assert!(
        *received,
        "Notification from the async tool should have been received even with a sync tool running."
    );
}
