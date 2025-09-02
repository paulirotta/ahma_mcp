//! Test for running clippy and fixing warnings.
use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use serde_json::Map;
use std::borrow::Cow;

mod common;
use common::test_client::new_client;

#[tokio::test]
async fn test_run_clippy() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("cargo_clippy"),
        arguments: None,
    };

    let result = client.call_tool(call_param).await?;

    // The result should indicate that the operation started.
    assert!(
        result
            .content
            .iter()
            .any(|c| c.as_text().is_some_and(|t| t.text.contains("started")))
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_run_clippy_with_tests() -> Result<()> {
    let client = new_client(Some("tools")).await?;

    let mut params = Map::new();
    params.insert("tests".to_string(), serde_json::Value::Bool(true));

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("cargo_clippy"),
        arguments: Some(params),
    };

    let result = client.call_tool(call_param).await?;

    // The result should indicate that the operation started.
    assert!(
        result
            .content
            .iter()
            .any(|c| c.as_text().is_some_and(|t| t.text.contains("started")))
    );

    client.cancel().await?;
    Ok(())
}
