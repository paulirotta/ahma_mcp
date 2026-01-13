# Ahma HTTP Bridge Requirements

Technical specification for the `ahma_http_bridge` component, which enables HTTP/SSE access to stdio-based MCP servers.

---

## 1. Core Mission

The Ahma HTTP Bridge provides a high-performance, non-blocking HTTP server that proxies JSON-RPC requests to a stdio-based MCP server subprocess. It expands the reach of MCP tools to web clients and environments where stdio transport is impractical.

## 2. Functional Requirements

### 2.1. Protocol Bridging

- **JSON-RPC Proxy**: Proxy JSON-RPC 2.0 requests from POST endpoints to the underlying stdio subprocess.
- **Server-Sent Events (SSE)**: Provide a persistent SSE stream at `/mcp` to broadcast notifications and keep-alive events from the MCP server to clients.
- **Content Negotiation**: Support `application/json` for single responses and `text/event-stream` for streaming, as per MCP specifications.

### 2.2. Session & Lifecycle Management

- **Process Manager**: Manage the full lifecycle of the MCP server subprocess, including auto-restarts on crash.
- **Session Isolation**: Support per-session sandbox isolation via the `--session-isolation` flag and `Mcp-Session-Id` header.
- **Deferred Initialization**: Ensure sessions are only marked ready after the `initialized` handshake is complete, preventing race conditions during restarts.

### 2.3. Security & Isolation

- **Sandbox Locking**: Lock the sandbox scope based on the first `roots/list` response (security invariant).
- **Immutable Scope**: Once a sandbox is locked to a workspace root, it cannot be changed for that session.
- **Local Dev Focus**: Optimized for local development; does not include built-in authentication for the bridge itself.

## 3. Technical Stack

- **Framework**: `axum` for the HTTP server.
- **Runtime**: `tokio` for async task orchestration and process I/O.
- **Concurrency**: `dashmap` for high-performance concurrent management of pending requests.
- **Middleware**: `tower-http` for CORS and structured tracing.

## 4. Constraints & Rules

### 4.1. Implementation Standards

- **Initialization Invariants**: Subprocesses must be fully initialized before processing client requests.
- **Atomic State Transitions**: Transitions from old to new subprocesses during restarts must be atomic.
- **Error Handling**: Bridge failures (e.g., subprocess crash) must return appropriate HTTP status codes (e.g., 403 Forbidden on sandbox violations, 500 on process failure).

### 4.2. Testing Philosophy

- **Minimum Coverage**: Target 80% line coverage for library code.
- **Isolated Testing**: Use `tower::ServiceExt::oneshot()` and mocked channels for endpoint verification.
- **Regression Testing**: Integration tests must cover the full process lifecycle and timeout scenarios.
- **Tooling**: Use `cargo llvm-cov` directly for coverage reports (avoiding sandbox interference).

## 5. User Journeys / Flows

### 5.1. Standard Request Flow

1. Client sends a POST request to `/mcp` with a JSON-RPC payload.
2. Bridge identifies the session and routes the request to the corresponding stdio subprocess.
3. Bridge waits (up to 60s) for the subprocess stdout response.
4. Bridge returns the response to the HTTP client.

### 5.2. SSE Notification Flow

1. Underlying MCP server writes a notification to its stdout.
2. Bridge captures the notification and broadcasts it to all connected SSE clients at `/mcp`.

### 5.3. Session Restart with Sandbox

1. A new session receives a workspace root via `roots/list`.
2. Bridge restarts the subprocess with the specific `--sandbox-scope`.
3. Bridge completes the `initialize`/`initialized` handshake with the new process.
4. Bridge resumes processing queued requests for that session.

## 6. Known Limitations & TODOs

- **Sensitive Path Restrictions**: Currently, broad read access is allowed. Future work involves `--deny-sensitive-reads` for paths like `~/.ssh`.
- **Network Restrictions**: Network access is currently unrestricted. Future work includes optional `--restrict-network` controls.
- **Temp Directory Cleanup**: Future investigation into ephemeral tmpfs overlays or per-session temp directories to prevent data persistence.
