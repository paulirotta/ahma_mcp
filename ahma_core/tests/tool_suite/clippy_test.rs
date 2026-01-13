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

    // Check if cargo tool is available (may not be in CI environment)
    let tools = client.list_all_tools().await?;
    if !tools.iter().any(|t| t.name == "cargo") {
        eprintln!("Skipping test: cargo tool not available (may be CI environment)");
        eprintln!(
            "Available tools: {:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        client.cancel().await?;
        return Ok(());
    }

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("cargo"),
        arguments: Some(serde_json::from_value(json!({ "subcommand": "clippy" })).unwrap()),
    };

    let result = client.call_tool(call_param).await?;

    // The result should be immediate since clippy is synchronous
    assert!(result.content.iter().any(|c| {
        c.as_text()
            .is_some_and(|t| t.text.contains("Finished") || t.text.contains("clippy"))
    }));

    client.cancel().await?;
    Ok(())
}
