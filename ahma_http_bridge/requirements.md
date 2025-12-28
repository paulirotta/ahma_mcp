# Ahma HTTP Bridge Requirements

## Overview

The Ahma HTTP Bridge provides an HTTP server that proxies JSON-RPC requests to a stdio-based MCP server subprocess. This enables HTTP clients to communicate with MCP servers that use stdio transport.

## Architecture

### Components

1. **BridgeConfig** - Configuration for bind address, server command, and arguments
2. **BridgeState** - Shared state including message channels and pending request tracking
3. **HTTP Endpoints**:
   - `GET /health` - Health check endpoint
   - `POST /mcp` - JSON-RPC request endpoint
   - `GET /mcp` - Server-Sent Events stream for notifications
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

## Deferred Sandbox & Restart

When a session is restarted (e.g., to apply a sandbox lock after receiving `roots/list`), the bridge must ensure the new subprocess is fully initialized before processing further requests.

### Initialization Flow

1. **Deferred Initialization**: The bridge marks a session as "initialized" only after the `initialized` notification has been successfully sent to the MCP server. This prevents race conditions where requests might be sent to an uninitialized server.
2. **Restart Handling**: When a session is restarted with a sandbox:
    - The bridge sends an `initialize` request to the new subprocess.
    - It waits for the `initialize` response.
    - It sends the `initialized` notification.
    - Only then is the session considered ready.

### Invariants

- **No Request Loss**: Client requests received during a restart are queued or handled gracefully.
- **Atomic Switch**: The session state transitions atomically from the old subprocess to the new one.

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

**Preferred for development in this repo**: run commands via `sandboxed_shell`.

Most other tool configs in `.ahma/tools/` (especially those marked `"enabled": false`) are considered deprecated; use the shell directly through the sandbox.

```bash
# Example quality pipeline
ahma_mcp sandboxed_shell --working-directory . -- \
    "cargo fmt --all && cargo clippy --all-targets && cargo test"

# You can also run tooling directly in a terminal if you prefer.
```

## API Reference

### POST /mcp

Accepts JSON-RPC 2.0 requests:

- Requests with `id` field wait for response (60s timeout)
- Notifications without `id` return immediately

### GET /mcp

Returns Server-Sent Events stream (per MCP Streamable HTTP spec):

- Broadcasts notifications from MCP server
- Keep-alive messages for connection health
- Same endpoint as POST, differentiated by HTTP method

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

## Future Security Enhancements (TODOs)

### 1. Optional Sensitive Path Restrictions (TODO)

Currently, the Seatbelt profile allows `file-read*` everywhere because shells and tools need to read from many system locations. This means AI can read sensitive files like `~/.ssh/id_rsa` or `~/.aws/credentials`.

**Future work:**

- Add optional `--deny-sensitive-reads` flag that blocks read access to sensitive paths
- Sensitive paths to consider: `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.config/gcloud`, browser credential stores
- Must be opt-in to avoid breaking legitimate tools that need these paths
- Consider allowing explicit allowlist overrides for specific use cases

### 2. Optional Network Traffic Restrictions (TODO)

Currently, network access is fully unrestricted (`(allow network*)` in Seatbelt profile). This allows potential data exfiltration via network.

**Future work:**

- Add optional `--restrict-network` flag that limits network access
- Must be coarse-grained (e.g., block all, allow localhost only, allow HTTP/HTTPS only)
- **Site-by-site allowlisting is security theater** - an attacker can use any allowed site as a proxy
- Real security requires either: (a) no network access, (b) proxy with content inspection, or (c) full outbound block with explicit exceptions
- Careful thought needed: many tools (package managers, git, language servers) need network access to function
- Consider separate profiles: "development" (network allowed) vs "offline" (network blocked)

### 3. Temp Directory Write Restrictions (TODO)

Currently `/tmp` and `/private/var/folders` are writable to support temp files from shells and tools. This creates a data persistence/exfiltration vector.

**Partial solution implemented:**

- `--no-temp-files` flag blocks writes to temp directories for higher security environments
- This breaks tools that use temp files (many do), so it's opt-in only

**Future investigation:**

- Can we isolate temp to per-session directories?
- Can we use a tmpfs overlay that's cleaned on session end?
- Trade-off: Many legitimate tools need temp file access
