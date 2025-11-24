//! # MCP Client for Testing
//!
//! This module provides a simple client wrapper for testing the MCP service.

use anyhow::Result;
use rmcp::{
    ServiceExt,
    model::CallToolRequestParam,
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use std::borrow::Cow;
use tokio::process::Command;

#[derive(Debug)]
pub struct Client {
    service: Option<RunningService<RoleClient, ()>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self { service: None }
    }

    pub async fn start_process(&mut self, tools_dir: Option<&str>) -> Result<()> {
        self.start_process_with_args(tools_dir, &[]).await
    }

    pub async fn start_process_with_args(
        &mut self,
        tools_dir: Option<&str>,
        extra_args: &[&str],
    ) -> Result<()> {
        let client = ()
            .serve(TokioChildProcess::new(Command::new("cargo").configure(
                |cmd| {
                    cmd.arg("run")
                        .arg("--package")
                        .arg("ahma_shell")
                        .arg("--bin")
                        .arg("ahma_mcp")
                        .arg("--");
                    if let Some(dir) = tools_dir {
                        cmd.arg("--tools-dir").arg(dir);
                    }
                    for arg in extra_args {
                        cmd.arg(arg);
                    }
                },
            ))?)
            .await?;
        self.service = Some(client);
        Ok(())
    }

    fn get_service(&self) -> Result<&RunningService<RoleClient, ()>> {
        self.service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not initialized"))
    }

    pub async fn shell_async_sleep(&mut self, duration: &str) -> Result<ToolCallResult> {
        let service = self.get_service()?;

        let params = CallToolRequestParam {
            name: Cow::Borrowed("bash"),
            arguments: Some(
                json!({
                    "subcommand": "default",
                    "args": [format!("sleep {}", duration)]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = service.call_tool(params).await?;
        if let Some(content) = result.content.first() {
            if let Some(text_content) = content.as_text() {
                // Parse the response text which is in format: "Asynchronous operation started with ID: {id}"
                // The response may also include tool hints after the ID, so extract just the ID
                let text = &text_content.text;
                if let Some(id_start) = text.find("ID: ") {
                    // Extract just the operation ID (everything after "ID: " until first whitespace or newline)
                    let id_text = &text[id_start + 4..];
                    let job_id = id_text
                        .split_whitespace()
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("Could not extract operation ID"))?
                        .to_string();
                    Ok(ToolCallResult {
                        status: "started".to_string(),
                        job_id,
                        message: text.clone(),
                    })
                } else {
                    Err(anyhow::anyhow!(
                        "Could not parse operation ID from response: {}",
                        text
                    ))
                }
            } else {
                Err(anyhow::anyhow!("No text content in response"))
            }
        } else {
            Err(anyhow::anyhow!("No text content in response"))
        }
    }

    pub async fn await_op(&mut self, op_id: &str) -> Result<String> {
        let service = self.get_service()?;

        let params = CallToolRequestParam {
            name: Cow::Borrowed("await"),
            arguments: Some(
                json!({
                    "operation_id": op_id
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = service.call_tool(params).await?;
        if result.content.is_empty() {
            return Err(anyhow::anyhow!("No content in response"));
        }

        // Concatenate all text content items
        let mut full_text = String::new();
        for content in &result.content {
            if let Some(text_content) = content.as_text() {
                if !full_text.is_empty() {
                    full_text.push_str("\n\n");
                }
                full_text.push_str(&text_content.text);
            }
        }

        if full_text.is_empty() {
            Err(anyhow::anyhow!("No text content in response"))
        } else {
            Ok(full_text)
        }
    }

    pub async fn status(&mut self, op_id: &str) -> Result<String> {
        let service = self.get_service()?;

        let params = CallToolRequestParam {
            name: Cow::Borrowed("status"),
            arguments: Some(
                json!({
                    "operation_id": op_id
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = service.call_tool(params).await?;
        if let Some(content) = result.content.first() {
            if let Some(text_content) = content.as_text() {
                Ok(text_content.text.clone())
            } else {
                Err(anyhow::anyhow!("No text content in response"))
            }
        } else {
            Err(anyhow::anyhow!("No text content in response"))
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct ToolCallResult {
    pub status: String,
    pub job_id: String,
    pub message: String,
}
