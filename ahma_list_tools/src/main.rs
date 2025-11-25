//! # Ahma List Tools
//!
//! A CLI utility to dump all MCP tool information from an MCP server to the terminal.
//! This is useful for tests, development, and verifying MCP server tool configurations.
//!
//! ## Usage
//!
//! Connect via command-line arguments:
//! ```bash
//! ahma_list_tools -- /path/to/ahma_mcp --tools-dir ./tools
//! ```
//!
//! Connect via mcp.json:
//! ```bash
//! ahma_list_tools --mcp-config /path/to/mcp.json --server Ahma
//! ```
//!
//! Connect to HTTP server:
//! ```bash
//! ahma_list_tools --http http://localhost:3000
//! ```

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use rmcp::{
    ServiceExt,
    model::Tool,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};
use tokio::process::Command;
use tracing::info;

/// CLI tool to list all tools from an MCP server
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Dump all MCP tool information to the terminal",
    long_about = "Connect to an MCP server (stdio or HTTP) and output all tool definitions in a human-readable format. Useful for testing, development, and verification of MCP server configurations."
)]
struct Cli {
    /// Path to mcp.json configuration file
    #[arg(long)]
    mcp_config: Option<PathBuf>,

    /// Name of the server in mcp.json to connect to (defaults to first server)
    #[arg(long)]
    server: Option<String>,

    /// HTTP URL of the MCP server (for HTTP mode)
    #[arg(long)]
    http: Option<String>,

    /// Output format: text (default) or json
    #[arg(long, default_value = "text")]
    format: OutputFormat,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Command and arguments for stdio MCP server (after --)
    #[arg(last = true)]
    command_args: Vec<String>,
}

#[derive(Debug, Clone, clap::ValueEnum, Default)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

/// MCP server configuration from mcp.json
#[derive(Debug, Deserialize, Serialize)]
struct McpConfig {
    servers: HashMap<String, ServerConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ServerConfig {
    #[serde(rename = "type")]
    server_type: String,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
}

/// Tool listing result for JSON output
#[derive(Debug, Serialize)]
struct ToolListResult {
    server_info: Option<ServerInfoOutput>,
    tools: Vec<ToolOutput>,
}

#[derive(Debug, Serialize)]
struct ServerInfoOutput {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct ToolOutput {
    name: String,
    description: Option<String>,
    parameters: Vec<ParameterOutput>,
}

#[derive(Debug, Serialize)]
struct ParameterOutput {
    name: String,
    param_type: String,
    required: bool,
    description: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.debug { "debug" } else { "warn" };
    ahma_core::utils::logging::init_logging(log_level, false)?;

    // Determine connection mode
    let result = if let Some(http_url) = &cli.http {
        list_tools_http(http_url).await?
    } else if let Some(mcp_config_path) = &cli.mcp_config {
        list_tools_from_config(mcp_config_path, cli.server.as_deref()).await?
    } else if !cli.command_args.is_empty() {
        list_tools_stdio(&cli.command_args).await?
    } else {
        return Err(anyhow!(
            "No connection method specified. Use --mcp-config, --http, or provide command after --"
        ));
    };

    // Output result
    match cli.format {
        OutputFormat::Text => print_text_output(&result),
        OutputFormat::Json => print_json_output(&result)?,
    }

    Ok(())
}

/// Parse mcp.json and get server configuration
fn parse_mcp_config(
    path: &PathBuf,
    server_name: Option<&str>,
) -> Result<(String, String, Vec<String>)> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read mcp.json from {}", path.display()))?;

    let config: McpConfig =
        serde_json::from_str(&content).with_context(|| "Failed to parse mcp.json")?;

    let (name, server_config): (String, ServerConfig) = if let Some(name) = server_name {
        let server = config
            .servers
            .get(name)
            .ok_or_else(|| anyhow!("Server '{}' not found in mcp.json", name))?;
        (name.to_string(), server.clone())
    } else {
        config
            .servers
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No servers defined in mcp.json"))?
    };

    let command = server_config
        .command
        .ok_or_else(|| anyhow!("Server '{}' has no command defined", name))?;

    let args = server_config.args.unwrap_or_default();

    // Expand home directory in command
    let command = expand_home(&command);
    let args: Vec<String> = args.iter().map(|a| expand_home(a)).collect();

    Ok((name, command, args))
}

/// Expand ~ to home directory
fn expand_home(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("~", home.to_str().unwrap_or("~"), 1);
    }
    path.to_string()
}

/// List tools from mcp.json configuration
async fn list_tools_from_config(
    config_path: &PathBuf,
    server_name: Option<&str>,
) -> Result<ToolListResult> {
    let (name, command, args) = parse_mcp_config(config_path, server_name)?;
    info!(
        "Connecting to server '{}' via stdio: {} {:?}",
        name, command, args
    );

    let mut all_args = vec![command];
    all_args.extend(args);

    list_tools_stdio(&all_args).await
}

/// List tools from stdio MCP server
async fn list_tools_stdio(command_args: &[String]) -> Result<ToolListResult> {
    if command_args.is_empty() {
        return Err(anyhow!("No command specified for stdio connection"));
    }

    let command = &command_args[0];
    let args = &command_args[1..];

    info!("Starting MCP server: {} {:?}", command, args);

    // Build command using rmcp's ConfigureCommandExt
    let args_clone: Vec<String> = args.to_vec();
    let transport = TokioChildProcess::new(Command::new(command).configure(move |cmd| {
        for arg in &args_clone {
            cmd.arg(arg);
        }
    }))?;

    let client = ().serve(transport).await?;

    // Get server info from peer_info
    let server_info_output = client.peer_info().map(|info| ServerInfoOutput {
        name: info.server_info.name.clone(),
        version: Some(info.server_info.version.clone()),
    });

    // List tools
    let tools_result = client.list_tools(None).await?;

    let tools: Vec<ToolOutput> = tools_result
        .tools
        .into_iter()
        .map(convert_tool_to_output)
        .collect();

    Ok(ToolListResult {
        server_info: server_info_output,
        tools,
    })
}

/// List tools from HTTP MCP server
async fn list_tools_http(url: &str) -> Result<ToolListResult> {
    // For now, use a simple HTTP client approach
    let client = reqwest::Client::new();

    // Send tools/list request
    let response = client
        .post(format!("{}/mcp", url.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .with_context(|| format!("Failed to connect to HTTP MCP server at {}", url))?;

    let response_text = response.text().await?;
    let response_json: serde_json::Value = serde_json::from_str(&response_text)
        .with_context(|| format!("Invalid JSON response: {}", response_text))?;

    // Extract tools from response
    let tools_array = response_json["result"]["tools"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid tools/list response format"))?;

    let tools: Vec<ToolOutput> = tools_array
        .iter()
        .map(|tool_json| {
            let name = tool_json["name"].as_str().unwrap_or("unknown").to_string();
            let description = tool_json["description"].as_str().map(|s| s.to_string());

            let parameters = extract_parameters_from_json(&tool_json["inputSchema"]);

            ToolOutput {
                name,
                description,
                parameters,
            }
        })
        .collect();

    Ok(ToolListResult {
        server_info: None,
        tools,
    })
}

fn extract_parameters_from_json(schema: &serde_json::Value) -> Vec<ParameterOutput> {
    let mut params = Vec::new();

    if let Some(properties) = schema["properties"].as_object() {
        let required_fields: Vec<&str> = schema["required"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        for (name, prop) in properties {
            let param_type = prop["type"].as_str().unwrap_or("string").to_string();
            let description = prop["description"].as_str().map(|s| s.to_string());
            let required = required_fields.contains(&name.as_str());

            params.push(ParameterOutput {
                name: name.clone(),
                param_type,
                required,
                description,
            });
        }
    }

    params
}

fn convert_tool_to_output(tool: Tool) -> ToolOutput {
    // input_schema is Arc<Map<String, Value>>, we need to extract properties and required from it
    let schema = &tool.input_schema;

    let properties = schema.get("properties").and_then(|v| v.as_object());

    let required_fields: Vec<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let parameters = if let Some(props) = properties {
        props
            .iter()
            .map(|(name, prop)| {
                let param_type = prop
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("string")
                    .to_string();

                let description = prop
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let required = required_fields.contains(&name.as_str());

                ParameterOutput {
                    name: name.clone(),
                    param_type,
                    required,
                    description,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    ToolOutput {
        name: tool.name.to_string(),
        description: tool.description.map(|s| s.to_string()),
        parameters,
    }
}

fn print_text_output(result: &ToolListResult) {
    println!("MCP Server Tools");
    println!("================");
    println!();

    if let Some(info) = &result.server_info {
        println!("Server: {}", info.name);
        if let Some(version) = &info.version {
            println!("Version: {}", version);
        }
        println!();
    }

    println!("Total tools: {}", result.tools.len());
    println!();

    for tool in &result.tools {
        println!("Tool: {}", tool.name);
        if let Some(desc) = &tool.description {
            println!("  Description: {}", desc);
        }
        if !tool.parameters.is_empty() {
            println!("  Parameters:");
            for param in &tool.parameters {
                let required_str = if param.required {
                    "required"
                } else {
                    "optional"
                };
                print!(
                    "    - {} ({}, {})",
                    param.name, param.param_type, required_str
                );
                if let Some(desc) = &param.description {
                    print!(": {}", desc);
                }
                println!();
            }
        }
        println!();
    }
}

fn print_json_output(result: &ToolListResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_home() {
        // Non-home path should not change
        assert_eq!(expand_home("/absolute/path"), "/absolute/path");
        assert_eq!(expand_home("relative/path"), "relative/path");

        // Home path should expand (if home dir exists)
        if dirs::home_dir().is_some() {
            let expanded = expand_home("~/test");
            assert!(!expanded.starts_with("~/"));
            assert!(expanded.ends_with("/test"));
        }
    }

    #[test]
    fn test_parse_mcp_config() {
        use std::io::Write;
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config_path = temp_dir.path().join("mcp.json");

        let config_content = r#"{
            "servers": {
                "TestServer": {
                    "type": "stdio",
                    "command": "/usr/bin/test",
                    "args": ["--flag", "value"]
                }
            }
        }"#;

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        let (name, command, args) = parse_mcp_config(&config_path, Some("TestServer")).unwrap();

        assert_eq!(name, "TestServer");
        assert_eq!(command, "/usr/bin/test");
        assert_eq!(args, vec!["--flag", "value"]);
    }

    #[test]
    fn test_parse_mcp_config_first_server() {
        use std::io::Write;
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config_path = temp_dir.path().join("mcp.json");

        let config_content = r#"{
            "servers": {
                "FirstServer": {
                    "type": "stdio",
                    "command": "/usr/bin/first"
                }
            }
        }"#;

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        let (name, command, _args) = parse_mcp_config(&config_path, None).unwrap();

        assert_eq!(name, "FirstServer");
        assert_eq!(command, "/usr/bin/first");
    }

    #[test]
    fn test_extract_parameters_from_json() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name"
                },
                "count": {
                    "type": "integer"
                }
            },
            "required": ["name"]
        });

        let params = extract_parameters_from_json(&schema);

        assert_eq!(params.len(), 2);

        let name_param = params.iter().find(|p| p.name == "name").unwrap();
        assert_eq!(name_param.param_type, "string");
        assert!(name_param.required);
        assert_eq!(name_param.description, Some("The name".to_string()));

        let count_param = params.iter().find(|p| p.name == "count").unwrap();
        assert_eq!(count_param.param_type, "integer");
        assert!(!count_param.required);
    }

    #[test]
    fn test_tool_output_serialization() {
        let tool = ToolOutput {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            parameters: vec![ParameterOutput {
                name: "param1".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("First param".to_string()),
            }],
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("test_tool"));
        assert!(json.contains("param1"));
    }
}
