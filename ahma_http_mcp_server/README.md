# Ahma HTTP MCP Server

**Status: Work in Progress**

A high-performance HTTP/3 and HTTP/2 server for the Model Context Protocol (MCP), designed for localhost development with automatic certificate management.

## Current Status

This crate is currently under development. The architecture has been designed but full integration with rmcp's service layer requires additional work to properly bridge HTTP requests to the MCP protocol handler.

## Planned Features

- **HTTP/3 Support**: Uses QUIC protocol for improved performance and multiplexing
- **HTTP/2 Fallback**: Automatically falls back to HTTP/2 if HTTP/3 is unavailable  
- **Zero Configuration**: Automatically generates and caches self-signed TLS certificates
- **Localhost Only**: Designed for secure local development
- **Server-Sent Events**: Supports SSE for server-initiated messages (HTTP/2 only)
- **Full MCP Support**: Complete implementation of the Model Context Protocol

## Architecture

The server is designed with the following components:

- `cert.rs` - Automatic self-signed certificate generation for localhost
- `error.rs` - Error types for the server
- `handler.rs` - MCP protocol handlers for HTTP requests
- `http2_server.rs` - HTTP/2 server using Axum
- `http3_server.rs` - HTTP/3 server using Quinn and h3
- `server.rs` - Main server orchestration with protocol selection

## Next Steps

To complete this implementation, the following work is needed:

1. Create a proper HTTP Transport that implements `rmcp::transport::Transport`
2. Integrate with rmcp's `ServiceExt::serve()` pattern
3. Handle JSON-RPC message routing through rmcp's service layer
4. Test with actual MCP clients

## Alternative Approach

For immediate HTTP access to MCP functionality, consider:

1. Using the existing stdio-based server with a reverse proxy
2. Creating an HTTP-to-stdio bridge
3. Using WebSocket transport (which rmcp may support)

## License

MIT OR Apache-2.0
