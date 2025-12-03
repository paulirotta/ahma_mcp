# Ahma HTTP Bridge Requirements

## Overview

The Ahma HTTP Bridge provides an HTTP server that proxies JSON-RPC requests to a stdio-based MCP server subprocess. This enables HTTP clients to communicate with MCP servers that use stdio transport.

## Architecture

### Components

1. **BridgeConfig** - Configuration for bind address, server command, and arguments
2. **BridgeState** - Shared state including message channels and pending request tracking
3. **HTTP Endpoints**:
   - `GET /health` - Health check endpoint
   - `POST /mcp` - JSON-RPC request/response endpoint
   - `GET /sse` - Server-Sent Events for real-time notifications
4. **Process Manager** - Manages MCP server subprocess lifecycle
5. **SessionManager** - Per-session subprocess isolation (optional)

## Session Isolation

When running with `--session-isolation` flag, the HTTP bridge supports per-session sandbox isolation:

### How It Works

1. **Client sends `initialize`** → Server generates session ID (UUID), spawns subprocess
2. **Server returns `Mcp-Session-Id` header** → Client includes on all subsequent requests
3. **Subprocess sends `roots/list` request** → Bridge locks sandbox scope from first root's URI
4. **Sandbox scope immutable** → Cannot change after first lock (security invariant)

### Key Behaviors

- Without `--session-isolation`: Single subprocess for all clients (default)
- With `--session-isolation`: Separate subprocess per MCP session
- Sandbox scope determined from first `roots/list` response
- `notifications/roots/list_changed` after lock → Session terminated (HTTP 403)
- HTTP DELETE with session ID → Clean session termination

### Security Model

- **Local development only** - No authentication
- **First-root wins** - Sandbox scope set once from first workspace root
- **Immutable after lock** - Prevents sandbox escape via workspace changes

See [docs/session-isolation.md](../docs/session-isolation.md) for detailed architecture.

## Testing Standards

### Coverage Goals

- **Target**: 80% line coverage for library code
- **Current**: 73.47% (as of 2025-11-25)
- `main.rs` is excluded from coverage goals (binary entry point)

### Coverage Tools

**llvm-cov is NOT available via Ahma MCP** because its instrumentation conflicts with macOS sandboxing.
To generate coverage reports, run `cargo llvm-cov` directly in your terminal:

```bash
# Generate HTML coverage report
cargo llvm-cov nextest --html --output-dir ./coverage

# Open in browser
open ./coverage/html/index.html
```

CI runs llvm-cov in the `job-coverage` workflow (GitHub Actions), where sandboxing is not enabled.

### Test Categories

1. **Unit Tests** - Test individual functions and components in isolation
2. **Integration Tests** - Test HTTP endpoints with mocked state
3. **Table-driven Tests** - Use parameterized tests for input/output variations

### Testing Patterns

#### HTTP Endpoint Testing

- Use `tower::ServiceExt::oneshot()` for single request tests
- Create test state with `mpsc::channel` for message passing
- Mock response channels with `oneshot::channel`

#### JSON-RPC Testing

- Test both string and numeric request IDs
- Test notifications (no ID) vs requests (with ID)
- Test error scenarios (channel closed, timeout)

#### SSE Testing

- Verify initial endpoint event
- Test broadcast message delivery

### Areas Currently Not Tested

The following require integration testing with a real subprocess:

- `manage_process()` - Full process lifecycle management
- `start_bridge()` - Server startup and binding
- Timeout scenarios (60s timeout on requests)

### Running Tests

**Always use Ahma MCP tools for Rust development** instead of running terminal commands directly:

- `mcp_ahma_cargo fmt` - Format code
- `mcp_ahma_cargo nextest_run` - Run tests
- `mcp_ahma_cargo clippy` - Lint code
- `mcp_ahma_cargo build` - Build project
- `mcp_ahma_ahma_quality_check` - Run full quality pipeline

The Ahma tools provide sandboxed execution, proper async handling, and integrated error reporting.

```bash
# Only use terminal directly for operations not supported by Ahma, such as:
cargo llvm-cov nextest --html  # Coverage (incompatible with sandboxing)
```

## API Reference

### POST /mcp

Accepts JSON-RPC 2.0 requests:

- Requests with `id` field wait for response (60s timeout)
- Notifications without `id` return immediately

### GET /sse

Returns Server-Sent Events stream:

- Initial `endpoint` event with value `/mcp`
- Broadcasts notifications from MCP server
- Keep-alive messages for connection health

### GET /health

Returns `200 OK` with body `OK` when server is healthy.

## Dependencies

### Runtime

- `axum` - HTTP framework
- `tokio` - Async runtime
- `dashmap` - Concurrent hash map for pending requests
- `tower-http` - CORS and tracing middleware

### Dev Dependencies

- `tower` (util feature) - `ServiceExt::oneshot()` for testing
