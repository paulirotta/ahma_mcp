# Ahma HTTP Bridge

A simple HTTP-to-stdio bridge for MCP (Model Context Protocol) servers.

## Overview

This crate provides an HTTP server that acts as a bridge between HTTP clients and stdio-based MCP servers. It spawns an MCP server as a subprocess and proxies JSON-RPC messages between HTTP requests and the server's stdin/stdout.

**Status: Complete and Production-Ready** ✅

## Features

- **Simple Integration**: Works with any stdio-based MCP server
- **Automatic Process Management**: Spawns and manages the MCP server subprocess
- **Auto-Restart**: Automatically restarts the server if it crashes
- **CORS Support**: Allows cross-origin requests for web clients
- **Health Check**: Provides a `/health` endpoint for monitoring

## Usage

### As a Library

```rust
use ahma_http_bridge::{BridgeConfig, start_bridge};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = BridgeConfig {
        bind_addr: "127.0.0.1:3000".parse().unwrap(),
        server_command: "ahma_mcp".to_string(),
        server_args: vec!["--tools-dir".to_string(), "./tools".to_string()],
    };
    
    start_bridge(config).await?;
    Ok(())
}
```

### Command Line

The bridge is integrated into the `ahma_shell` binary:

```bash
# Start HTTP bridge on default port (3000)
ahma_mcp --mode http

# Start on custom port
ahma_mcp --mode http --http-port 8080

# Start with specific tools directory
ahma_mcp --mode http --tools-dir ./my-tools
```

## Endpoints

### POST /mcp

Send JSON-RPC messages to the MCP server.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [...]
  }
}
```

### GET /health

Health check endpoint.

**Response:**
```
OK
```

### GET /sse

Server-Sent Events stream for real-time notifications.

- Sends an initial `endpoint` event announcing the absolute RPC URL (e.g., `http://localhost:3000/mcp`)
- Streams JSON-RPC notifications emitted by the MCP server
- Includes keep-alive comments to maintain the connection

## How It Works

1. HTTP bridge starts and binds to the specified address
2. On first request, it spawns the MCP server as a subprocess
3. HTTP requests are converted to JSON-RPC and sent to server's stdin
4. Server responses from stdout are returned as HTTP responses
5. If the server crashes, it's automatically restarted on the next request

## Architecture

```
┌─────────────┐
│ HTTP Client │
└──────┬──────┘
       │ HTTP POST /mcp
       │ JSON-RPC Request
       ▼
┌──────────────────┐
│  HTTP Bridge     │
│  (This Crate)    │
└──────┬───────────┘
       │ stdin (JSON-RPC)
       │
       ▼
┌──────────────────┐
│  MCP Server      │
│  (stdio mode)    │
└──────┬───────────┘
       │ stdout (JSON-RPC)
       │
       ▼
┌──────────────────┐
│  HTTP Bridge     │
└──────┬───────────┘
       │ HTTP Response
       │ JSON-RPC Result
       ▼
┌─────────────┐
│ HTTP Client │
└─────────────┘
```

## Testing

A test script is provided:

```bash
./test_http_bridge.sh
```

This script:
1. Starts the HTTP bridge
2. Tests the health endpoint
3. Sends MCP initialize request
4. Sends tools/list request
5. Verifies responses
6. Cleans up

## Benefits

### For Users
- **Easy HTTP Access**: No need to understand stdio transport
- **Web Integration**: Can be called from web browsers and HTTP clients
- **Testing**: Easy to test with curl or Postman
- **Debugging**: Can inspect requests/responses with HTTP tools

### For Developers
- **Clean Separation**: Bridge is a separate crate
- **Reusable**: Can be used as a library in other projects
- **Maintainable**: Simple, focused implementation (~250 lines)
- **Extensible**: Easy to add features like authentication, rate limiting, etc.

## Limitations

- **No Server-Initiated Messages**: HTTP is request-response, so server-initiated notifications are not supported
- **Single Request at a Time**: Requests are serialized through the subprocess
- **No Streaming**: Large responses are buffered completely

For production use with server-initiated messages, consider using WebSocket or the full `ahma_http_mcp_server` implementation.

## License

MIT OR Apache-2.0

