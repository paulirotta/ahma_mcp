//! Test for running clippy and fixing warnings.
mod common;
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::json;
use std::borrow::Cow;

use ahma_core::utils::logging::init_test_logging;
use common::test_client::new_client;

#[tokio::test]
async fn test_run_clippy() -> Result<()> {
    init_test_logging();
    let client = new_client(Some(".ahma/tools")).await?;

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
