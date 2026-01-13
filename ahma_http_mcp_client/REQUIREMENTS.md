# Ahma HTTP MCP Client Requirements

Technical specification for the `ahma_http_mcp_client` library, which provides the client-side transport layer for communicating with MCP servers over HTTP/SSE.

---

## 1. Core Mission

The `ahma_http_mcp_client` library enables Rust applications to act as MCP clients for remote servers exposed via HTTP. It handles the complexities of Server-Sent Events (SSE) for server-to-client messaging, standard HTTP POST requests for client-to-server commands, and integrates OAuth2 authentication flows.

## 2. Functional Requirements

### 2.1. Protocol Transport

-   **Transport Layer**: Implement the `mcp_client::Transport` trait (or equivalent) to integrate seamlessly with the MCP SDK.
-   **SSE Consumption**: Connect to SSE endpoints (e.g., `/sse` or `/mcp`) to receive JSON-RPC notifications and responses.
-   **Command Dispatch**: Send JSON-RPC requests via HTTP POST to the endpoint specified by the SSE initialization event.
-   **Async Stream**: Expose incoming messages as a `Stream` of `McpMessage` objects.

### 2.2. Authentication

-   **OAuth2 Integration**: Support standard OAuth2 authorization code flows.
-   **Token Management**: Handle refreshing of access tokens where applicable.
-   **Header Injection**: Automatically inject `Authorization: Bearer <token>` headers into requests.

### 2.3. Connection Management

-   **Handshake Handling**: Correctly parse the initial endpoint discovery from the SSE stream.
-   **Reconnection**: (Future/Optional) Logic to handle dropped SSE connections gracefully.
-   **Error Propagation**: Convert HTTP errors (4xx, 5xx) and transport failures into typed MCP errors.

## 3. Technical Stack

-   **HTTP Client**: `reqwest` for robust async HTTP operations.
-   **Auth**: `oauth2` crate for handling standard flows.
-   **Async Runtime**: `tokio` for managing the event loop.
-   **Streaming**: `futures` and `async-stream` for SSE processing.
-   **Protocol**: `rmcp` (or shared core) for MCP types and traits.

## 4. Constraints & Rules

### 4.1. Implementation Standards

-   **Non-Blocking**: All I/O operations must be non-blocking async.
-   **Type Safety**: Use strong typing for URLs, tokens, and configuration.
-   **Security**: Do not log sensitive token data; use `tracing` with appropriate levels.

### 4.2. Testing Philosophy

-   **Minimum Coverage**: Target 80% line coverage for library code.
-   **Mocking**: Use `wiremock` to simulate MCP servers and OAuth endpoints for reliable integration testing.
-   **Resilience**: Tests must cover network timeout and invalid payload scenarios.

## 5. User Journeys / Flows

### 5.1. Basic Connection

1.  User instantiates `HttpMcpTransport` with a base URL (e.g., `https://api.example.com/sse`).
2.  Client initiates SSE connection.
3.  Server sends `endpoint` event pointing to command URL (e.g., `/messages`).
4.  Client is ready to send requests to `/messages` and receive events via the open stream.

### 5.2. OAuth2 Connection

1.  User provides Client ID, Client Secret, and Auth URL.
2.  Client initiates OAuth dance (opening browser if necessary via `webbrowser`).
3.  On success, client receives Access Token.
4.  Client connects to MCP server using the token for both SSE and POST requests.

## 6. Known Limitations & TODOs

-   **Token Persistence**: No built-in storage for tokens across process restarts; relies on the consumer or re-authentication.
-   **Legacy Protocol**: Primary focus is latest MCP spec; legacy adaptations may be limited.
