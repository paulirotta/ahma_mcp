//! Integration tests for the ahma_mcp service.
mod common;

use ahma_mcp::config::Config;
use ahma_mcp::mcp_service::AhmaMcpService;
use anyhow::Result;
use common::{create_test_config, get_workspace_dir};
use rmcp::model::{CallToolRequestParam, NumberOrString};
use rmcp::service::{RequestContext, RoleServer, Service};
use rmcp::{ErrorData as RmcpError, ServerHandler, model as m, service};
use serde_json::Map;
use std::borrow::Cow;
use tokio::io::duplex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// A dummy service that does nothing, just to get a Peer instance.
#[derive(Clone)]
struct DummyService;

impl Service<RoleServer> for DummyService {
    async fn handle_request(
        &self,
        _request: m::ClientRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<m::ServerResult, RmcpError> {
        unimplemented!()
    }

    async fn handle_notification(
        &self,
        _notification: m::ClientNotification,
        _context: rmcp::service::NotificationContext<RoleServer>,
    ) -> Result<(), RmcpError> {
        Ok(())
    }

    fn get_info(&self) -> <RoleServer as rmcp::service::ServiceRole>::Info {
        unimplemented!()
    }
}

fn dummy_peer() -> (rmcp::service::Peer<RoleServer>, tokio::task::JoinHandle<()>) {
    let (_client_transport, server_transport) = duplex(1024);

    let running_service = service::serve_directly(DummyService, server_transport, None);
    let peer = running_service.peer().clone();

    let handle = tokio::spawn(async move {
        let _ = running_service.waiting().await;
    });

    (peer, handle)
}

fn dummy_request_context() -> (RequestContext<RoleServer>, tokio::task::JoinHandle<()>) {
    let (peer, handle) = dummy_peer();
    let request_context = RequestContext {
        id: NumberOrString::String(Uuid::new_v4().to_string().into()),
        peer,
        ct: CancellationToken::new(),
        meta: Default::default(),
        extensions: Default::default(),
    };
    (request_context, handle)
}

fn create_test_configs() -> Vec<(String, Config)> {
    let echo_config = Config {
        command: "echo".to_string(),
        subcommand: vec![ahma_mcp::config::Subcommand {
            name: "text".to_string(),
            description: "Echo text to output".to_string(),
            options: vec![ahma_mcp::config::CliOption {
                name: "message".to_string(),
                type_: "string".to_string(),
                description: "Message to echo".to_string(),
            }],
            args: vec![],
            synchronous: Some(true), // Explicitly synchronous for testing
        }],
        hints: None,
        enabled: Some(true),
    };

    vec![("echo".to_string(), echo_config)]
}

#[tokio::test]
async fn test_service_creation() -> Result<()> {
    let _workspace_dir = get_workspace_dir();
    let adapter = create_test_config(&_workspace_dir)?;
    let configs = create_test_configs();

    let _service = AhmaMcpService::new(adapter, configs).await?;

    // Just verify it was created successfully
    assert!(true, "Service creation should succeed");

    Ok(())
}

#[tokio::test]
async fn test_list_tools() -> Result<()> {
    let _workspace_dir = get_workspace_dir();
    let adapter = create_test_config(&_workspace_dir)?;
    let configs = create_test_configs();

    let service = AhmaMcpService::new(adapter, configs).await?;
    let (request_context, handle) = dummy_request_context();

    let result = service.list_tools(None, request_context).await?;

    handle.abort();

    // Should have echo_text tool
    assert_eq!(result.tools.len(), 1);
    let tool_names: Vec<_> = result.tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(tool_names.contains(&"echo_text"));

    Ok(())
}

#[tokio::test]
async fn test_call_tool_basic() -> Result<()> {
    let workspace_dir = get_workspace_dir();
    let adapter = create_test_config(&workspace_dir)?;
    let configs = create_test_configs();

    let service = AhmaMcpService::new(adapter, configs).await?;
    let (request_context, handle) = dummy_request_context();

    let mut params = Map::new();
    params.insert(
        "working_directory".to_string(),
        serde_json::Value::String(workspace_dir.to_string_lossy().to_string()),
    );
    params.insert(
        "message".to_string(),
        serde_json::Value::String("Hello from test".to_string()),
    );

    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("echo_text"),
        arguments: Some(params),
    };

    let result = service.call_tool(call_param, request_context).await?;

    handle.abort();

    // The result should contain our echo message
    assert!(!result.content.is_empty());
    if let Some(content) = result.content.first()
        && let Some(text_content) = content.as_text()
    {
        assert!(text_content.text.contains("Hello from test"));
    }

    Ok(())
}
