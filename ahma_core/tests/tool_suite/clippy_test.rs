//! Test for running clippy and fixing warnings.
use ahma_core::test_utils as common;
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

use ahma_core::utils::logging::init_test_logging;
use common::test_client::new_client;

#[tokio::test]
async fn test_run_clippy() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma")).await?;

    // Check if sandboxed_shell tool is available
    let tools = client.list_all_tools().await?;
    if !tools.iter().any(|t| t.name == "sandboxed_shell") {
        eprintln!("Skipping test: sandboxed_shell tool not available");
        client.cancel().await?;
        return Ok(());
    }

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("sandboxed_shell"),
        arguments: Some(
            serde_json::from_value(json!({
                "command": "cargo clippy"
            }))
            .unwrap(),
        ),
        task: None,
    };

    let result = client.call_tool(call_param).await?;

    // Extract operation ID from async result
    let op_id = result.content.iter().find_map(|c| {
        c.as_text().and_then(|t| {
            t.text
                .split("ID: ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .map(String::from)
        })
    });

    if let Some(operation_id) = op_id {
        // Await for the operation to complete
        let await_param = CallToolRequestParam {
            name: Cow::Borrowed("await"),
            arguments: Some(
                serde_json::from_value(json!({
                    "operation_id": operation_id
                }))
                .unwrap(),
            ),
            task: None,
        };

        let await_result = client.call_tool(await_param).await?;

        // The result should contain clippy output
        assert!(
            await_result.content.iter().any(|c| {
                c.as_text().is_some_and(|t| {
                    t.text.contains("Finished")
                        || t.text.contains("clippy")
                        || t.text.contains("Checking")
                })
            }),
            "Expected clippy output in await result"
        );
    } else {
        panic!("Expected operation ID in async result");
    }

    client.cancel().await?;
    Ok(())
}
