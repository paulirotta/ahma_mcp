# Ahma MCP Requirements

Technical specification for the `ahma_mcp` project. For AI development instructions, see [AGENTS.md](AGENTS.md).

---

## 1. Core Mission

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for use by AI agents. Its purpose is to provide a consistent, powerful, and non-blocking bridge between AI and the vast ecosystem of command-line utilities.

## 2. Functional Requirements

### 2.1. Dynamic Tool Adaptation
- **Declarative Configuration**: Adapt any CLI tool via JSON files in the `tools/` directory. No recompilation required to add or modify tools.
- **Hot-Reloading**: Automatically watch the `tools/` directory for changes and notify clients via `notifications/tools/list_changed`.
- **MTDF Support**: Implement comprehensive validation against the MCP Tool Definition Format (MTDF) schema, supporting types, required fields, and path formats.

### 2.2. Execution Engine
- **Async-First Execution**: Operations execute asynchronously by default, returning an `operation_id` immediately and pushing results via MCP progress notifications.
- **Selective Sync Override**: Support `"synchronous": true` for critical operations (e.g., dependency management) to prevent race conditions.
- **Performance**: Use a pre-warmed shell pool to achieve command startup latencies of 5-20ms.
- **Sequence Tooling**: Support chaining multiple commands into single workflows via top-level or subcommand-level sequences.

### 2.3. Connectivity & Integration
- **HTTP Bridge**: Expose the server over HTTP/SSE (`ahma_mcp --mode http`) with support for concurrent requests and auto-restart of the stdio subprocess.
- **Content Negotiation**: Support both `application/json` and `text/event-stream` (SSE) based on the `Accept` header.
- **Session Isolation**: Support per-session sandbox isolation using the `Mcp-Session-Id` header (optional flag).
- **HTTP MCP Client**: Ability to act as a client for remote HTTP-based MCP servers with OAuth 2.0 (PKCE) authentication support.
- **List-Tools Mode**: Output available tools as JSON via CLI (`ahma_mcp --mode list-tools`).

### 2.4. Reliability & Stability
- **Cancellation Handling**: Distinguish between protocol-level and process-level cancellations. Cancel background shell processes while maintaining stability of synchronous control calls.
- **Unified Output**: Redirect stderr to stdout (`2>&1`) for shell commands to provide a single, chronologically ordered stream to AI clients.

## 3. Technical Stack

- **Language**: Rust
- **Project Structure**: Cargo workspace with modular crates:
    - `ahma_core`: Protocol-agnostic core library and main `ahma_mcp` binary.
    - `ahma_http_bridge`: HTTP/SSE bridge implementation.
    - `ahma_http_mcp_client`: OAuth-enabled HTTP client (pending full integration).
    - `ahma_validate`: CLI for tool configuration validation.
    - `generate_tool_schema`: MTDF JSON Schema generation.
    - `ahma_list_tools`: Tool inspection utility.
- **Core Dependencies**:
    - `rmcp`: MCP protocol implementation.
    - `tokio`: Async runtime for non-blocking I/O and process management.
    - `serde` / `serde_json`: Serialization and schema handling.
    - `anyhow`: Error propagation.
    - `tracing`: Structured logging.
- **Sandboxing Technology**:
    - **Linux**: Landlock (kernel 5.13+).
    - **macOS**: Seatbelt (sandbox-exec) with SBPL profiles.

## 4. Constraints & Rules

### 4.1. Runtime Terminology
- **Ahama**: The name of the live MCP service as defined in `mcp.json`.
- **ahma_mcp**: The name of the project, repository, and compiled binary.
- Clear distinction must be maintained between the running service and the source project.

### 4.2. Security Invariants
- **Kernel-Enforced Sandboxing**: Use Landlock (Linux) or Seatbelt (macOS) to restrict file system access.
- **Sandbox Scope**: Fixed at initialization; cannot be changed during a session.
- **Strict SBPL Syntax**: (macOS specific) Path filters in Seatbelt profiles must be on the same line to avoid `SIGABRT`.
- **Fail-Secure**: Refuse to start if sandboxing prerequisites are missing, or if running in a nested environment without explicit `--no-sandbox` override.

### 4.3. Implementation Standards
- **Async I/O Hygiene**: `std::fs` and `std::io` are forbidden in async contexts; use `tokio::fs` and `tokio::io`.
- **Meta-Parameter Filtering**: `working_directory`, `execution_mode`, and `timeout_seconds` must be filtered out before passing arguments to CLI tools.
- **Error Handling**: Internal errors use `anyhow::Result`; external communication uses `McpError` with actionable context.
- **Logging**: Default to daily rolling files in the cache directory. Support `--log-to-stderr` for development with ANSI colors (Posix only).

### 4.4. Testing Philosophy
- **Mandatory Coverage**: All new functionality must include unit, integration, or regression tests.
- **Isolated Workspace**: All tests must use `tempfile` to prevent repository pollution.
- **CLI Integration**: Binaries must have end-to-end tests for help, version, and basic functional paths.

## 5. User Journeys / Flows

### 5.1. AI Agent Workflow (Asynchronous)
1. AI invokes a tool (e.g., `cargo_test`).
2. Server returns `operation_id` and status `started` immediately.
3. AI continues with other tasks (reading docs, planning next steps).
4. Operation completes; server pushes result via `notifications/progress`.
5. AI processes the result to determine the outcome.

### 5.2. Tool Developer Workflow
1. Developer adds a new `.json` definition to `tools/`.
2. `ahma_mcp` detects the change via file watcher.
3. Server validates the new config against MTDF schema.
4. Server notifies the IDE/Client that tools have changed.
5. The new tool is immediately available for AI use without a restart.

### 5.3. Remote Integration (HTTP Client)
1. `ahma_mcp` is configured to connect to a remote Atlassian server.
2. User authenticates via browser (OAuth PKCE).
3. `ahma_mcp` stores the token and initiates a persistent SSE connection.
4. Remote tools are proxied through `ahma_mcp` to the local AI client.

## 6. Known Limitations

- **Nested Subcommands**: Limited testing for nesting levels > 2.
- **Cross-Platform**: Windows support is secondary to macOS/Linux.
- **Session Isolation**: HTTP mode currently defaults to a single shared sandbox instance unless the isolation flag is active.
- **Retry Logic**: No built-in retry mechanism for transient process failures.
