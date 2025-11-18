# Implementation Notes for ahma_http_mcp_server

## What Has Been Created

### 1. Project Structure
- ✅ Created new crate `ahma_http_mcp_server` in the workspace
- ✅ Added comprehensive dependencies in Cargo.toml for HTTP/3, HTTP/2, and TLS
- ✅ Set up proper workspace integration

### 2. Certificate Management (`cert.rs`)
- ✅ Automatic self-signed certificate generation using `rcgen`
- ✅ Certificate caching in `~/.ahma_mcp_certs/`
- ✅ Support for localhost with proper SANs (DNS + IPv4 + IPv6)
- ✅ PEM parsing utilities for rustls integration

### 3. Error Handling (`error.rs`)
- ✅ Comprehensive error types for HTTP/2, HTTP/3, certificates, and MCP
- ✅ Proper error conversion with thiserror

### 4. Server Architecture (`server.rs`)
- ✅ Protocol selection enum (HTTP/2, HTTP/3, HTTP/3WithFallback, Both)
- ✅ Server configuration structure
- ✅ Orchestration logic for running servers

### 5. HTTP/2 Server (`http2_server.rs`)
- ✅ Axum-based HTTP/2 server
- ✅ TLS configuration with auto-generated certificates
- ✅ Plaintext mode for testing
- ✅ CORS and tracing middleware

### 6. HTTP/3 Server (`http3_server.rs`)
- ✅ Quinn + h3 based HTTP/3 server
- ✅ QUIC transport configuration
- ✅ TLS with ALPN for h3

### 7. CLI (`main.rs`)
- ✅ Comprehensive command-line interface with clap
- ✅ Protocol selection
- ✅ Address configuration
- ✅ Logging configuration

## What Needs to Be Completed

### 1. rmcp Integration Challenge

The main blocker is properly integrating with rmcp's service architecture. The `rmcp` crate uses a specific pattern:

```rust
handler.serve(transport).await
```

Where:
- `handler` implements `ServerHandler` trait
- `transport` implements `Transport<RoleServer>` trait

**Problem**: We're trying to manually handle JSON-RPC messages, but rmcp expects to control the message flow through its service layer.

**Solutions to explore**:

#### Option A: Create HTTP Transport (Recommended)
Create a struct that implements `rmcp::transport::Transport<RoleServer>`:

```rust
pub struct HttpTransport {
    // HTTP request/response channels
    // Connection management
}

impl Transport<RoleServer> for HttpTransport {
    type Error = ServerError;
    
    fn send(&mut self, item: TxJsonRpcMessage<RoleServer>) -> impl Future<...> {
        // Send JSON-RPC message as HTTP response
    }
    
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        // Receive JSON-RPC message from HTTP request
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        // Close connection
    }
}
```

Then use it like:
```rust
let transport = HttpTransport::new(/* ... */);
let service = handler.serve(transport).await?;
service.run().await?;
```

#### Option B: HTTP-to-Stdio Bridge
Create an HTTP server that spawns the stdio-based MCP server as a subprocess and proxies requests:

```rust
// HTTP request -> JSON-RPC -> stdin of subprocess
// stdout of subprocess -> JSON-RPC -> HTTP response
```

This is simpler but less efficient.

#### Option C: Use WebSocket
Check if rmcp has WebSocket transport support, which would be more natural for bidirectional MCP communication.

### 2. Specific Code Issues to Fix

#### In `handler.rs` and `http3_server.rs`:
- Remove manual JSON-RPC message handling
- Don't call `handler.call()` directly (Service trait is not the right approach)
- Instead, create proper Transport implementation

#### In `http3_server.rs`:
- Fix h3 API usage (the accept() method returns a different type in newer versions)
- Handle request/response streaming properly with h3's API

#### General:
- The `ServerHandler` trait methods (`get_info`, `list_tools`, `call_tool`) should be called by rmcp's service layer, not directly by our HTTP handlers
- We need to let rmcp handle the JSON-RPC protocol details

### 3. Testing Strategy

Once the Transport is implemented:

1. Unit tests for certificate generation
2. Integration tests for HTTP/2 and HTTP/3 servers
3. End-to-end tests with actual MCP clients
4. Performance benchmarks comparing HTTP/2 vs HTTP/3

### 4. Documentation

- API documentation for all public types
- Examples of using the server
- Guide for accepting self-signed certificates
- Performance tuning guide

## Recommended Next Steps

1. **Study rmcp's Transport trait** - Look at how `stdio()` transport is implemented
2. **Create HttpTransport** - Implement Transport trait for HTTP
3. **Handle bidirectional communication** - Figure out how to handle server-initiated messages (SSE for HTTP/2, or long-polling)
4. **Test with mcp-inspector** - Use the MCP inspector tool to verify protocol compliance
5. **Add examples** - Create example clients showing how to connect

## References

- [MCP Specification](https://modelcontextprotocol.io/)
- [rmcp crate documentation](https://docs.rs/rmcp/)
- [Quinn documentation](https://docs.rs/quinn/)
- [Axum documentation](https://docs.rs/axum/)
- [h3 documentation](https://docs.rs/h3/)

## Notes on Dependencies

- `quinn 0.11` - QUIC implementation
- `h3 0.0.8` - HTTP/3 implementation
- `h3-quinn 0.0.10` - Quinn integration for h3
- `axum 0.7` - HTTP/2 web framework
- `rcgen 0.13` - Certificate generation
- `rustls 0.23` - TLS implementation

All dependencies are compatible and up-to-date as of the implementation date.

