//! Integration tests for the ahma_mcp service.
mod common;

use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::Map;
use std::borrow::Cow;

#[tokio::test]
async fn test_list_tools() -> Result<()> {
    let client = new_client(Some("tools")).await?;
    let result = client.list_all_tools().await?;

    // Should have at least the built-in 'wait' tool
    assert!(!result.is_empty());
    let tool_names: Vec<_> = result.iter().map(|t| t.name.as_ref()).collect();
    assert!(tool_names.contains(&"wait"));
    assert!(tool_names.contains(&"ls_default"));

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
        name: Cow::Borrowed("ls_default"),
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
