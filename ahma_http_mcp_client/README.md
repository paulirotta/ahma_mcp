# Ahma HTTP MCP Client

HTTP client library for communicating with MCP servers over HTTP.

## Overview

This crate provides a client for interacting with MCP servers via HTTP transport, supporting both the HTTP bridge and native HTTP MCP servers.

## Features

- HTTP/HTTPS support
- JSON-RPC message handling
- Async/await API
- Error handling and retries

## Usage

```rust
use ahma_http_mcp_client::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new("http://localhost:3000")?;
    
    // Send MCP request
    let response = client.send_request("tools/list", serde_json::json!({})).await?;
    
    Ok(())
}
```

## Examples

### Atlassian Client

An example client that connects to the Atlassian MCP server using OAuth2 is provided in `examples/atlassian_client.rs`.

To run it:

```bash
cargo run --example atlassian_client -- --client-id YOUR_CLIENT_ID --client-secret YOUR_CLIENT_SECRET
```

## License

MIT OR Apache-2.0
