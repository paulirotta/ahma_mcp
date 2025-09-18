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
        let client = ()
            .serve(TokioChildProcess::new(Command::new("cargo").configure(
                |cmd| {
                    cmd.arg("run").arg("--bin").arg("ahma_mcp").arg("--");
                    if let Some(dir) = tools_dir {
                        cmd.arg("--tools-dir").arg(dir);
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

    pub async fn long_running_async(&mut self, duration: &str) -> Result<ToolCallResult> {
        let service = self.get_service()?;

        let params = CallToolRequestParam {
            name: Cow::Borrowed("long_running_async"),
            arguments: Some(
                json!({
                    "duration": duration
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = service.call_tool(params).await?;
        if let Some(content) = result.content.first() {
            if let Some(text_content) = content.as_text() {
                serde_json::from_str(&text_content.text).map_err(|e| anyhow::anyhow!(e))
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
