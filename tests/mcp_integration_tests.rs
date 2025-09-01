//! Integration tests for the ahma_mcp service.
mod common;

use ahma_mcp::config::{OptionConfig, SubcommandConfig, ToolConfig};
use ahma_mcp::mcp_service::AhmaMcpService;
use anyhow::Result;
use common::{create_test_config, get_workspace_dir};
use rmcp::model::{CallToolRequestParam, NumberOrString};
use rmcp::service::{RequestContext, RoleServer, Service};
use rmcp::{ErrorData as RmcpError, ServerHandler, model as m, service};
use serde_json::{Map, json};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::duplex;
use tokio_util::sync::CancellationToken;

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
        id: NumberOrString::String("test_req_1".to_string().into()),
        peer,
        ct: CancellationToken::new(),
        meta: Default::default(),
        extensions: Default::default(),
    };
    (request_context, handle)
}

fn create_test_configs() -> HashMap<String, ToolConfig> {
    let mut configs = HashMap::new();
    let echo_config = ToolConfig {
        name: "echo".to_string(),
        description: "Echo text to output".to_string(),
        command: "echo".to_string(),
        subcommand: vec![SubcommandConfig {
            name: "text".to_string(),
            description: "Echo text to output".to_string(),
            options: vec![OptionConfig {
                name: "message".to_string(),
                option_type: "string".to_string(),
                description: "Message to echo".to_string(),
            }],
            synchronous: Some(true), // Fast echo command should be synchronous
            timeout_seconds: Some(30),
            hint: Some("Echo is fast and synchronous - result returns immediately.".to_string()),
        }],
        input_schema: json!({}),
        timeout_seconds: Some(30),
        hints: Default::default(),
        enabled: true,
    };

    configs.insert("echo".to_string(), echo_config);
    configs
}

#[tokio::test]
async fn test_service_creation() -> Result<()> {
    let _workspace_dir = get_workspace_dir();
    let adapter = create_test_config(&_workspace_dir)?;

    // Create a separate operation monitor for the service
    let monitor_config = ahma_mcp::operation_monitor::MonitorConfig::with_timeout(
        std::time::Duration::from_secs(30),
    );
    let operation_monitor = Arc::new(ahma_mcp::operation_monitor::OperationMonitor::new(
        monitor_config,
    ));

    let configs = Arc::new(create_test_configs());

    let _service = AhmaMcpService::new(adapter, operation_monitor, configs).await?;

    // Just verify it was created successfully
    assert!(true, "Service creation should succeed");

    Ok(())
}

#[tokio::test]
async fn test_list_tools() -> Result<()> {
    let _workspace_dir = get_workspace_dir();
    let adapter = create_test_config(&_workspace_dir)?;

    // Create a separate operation monitor for the service
    let monitor_config = ahma_mcp::operation_monitor::MonitorConfig::with_timeout(
        std::time::Duration::from_secs(30),
    );
    let operation_monitor = Arc::new(ahma_mcp::operation_monitor::OperationMonitor::new(
        monitor_config,
    ));

    let configs = Arc::new(create_test_configs());

    let service = AhmaMcpService::new(adapter, operation_monitor, configs).await?;
    let (request_context, handle) = dummy_request_context();

    let result = service.list_tools(None, request_context).await?;

    handle.abort();

    // Should have echo_text tool (base command "echo" + subcommand "text") AND wait tool (hard-wired)
    assert_eq!(result.tools.len(), 2);
    let tool_names: Vec<_> = result.tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(tool_names.contains(&"echo_text"));
    assert!(tool_names.contains(&"wait"));

    Ok(())
}

#[tokio::test]
async fn test_call_tool_basic() -> Result<()> {
    let workspace_dir = get_workspace_dir();
    let adapter = create_test_config(&workspace_dir)?;

    // Create a separate operation monitor for the service
    let monitor_config = ahma_mcp::operation_monitor::MonitorConfig::with_timeout(
        std::time::Duration::from_secs(30),
    );
    let operation_monitor = Arc::new(ahma_mcp::operation_monitor::OperationMonitor::new(
        monitor_config,
    ));

    let configs = Arc::new(create_test_configs());

    let service = AhmaMcpService::new(adapter, operation_monitor, configs).await?;
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
