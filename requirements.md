# Ahma MCP Requirements

This document is the single source of truth for the `ahma_mcp` project. It outlines the core requirements, architecture, and principles that an AI maintainer must follow. All new development tasks will be reflected as changes in this document.

## 1. Core Mission

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for use by AI agents. Its purpose is to provide a consistent, powerful, and non-blocking bridge between AI and the vast ecosystem of command-line utilities.

## 2. Core Principles & Requirements

These are the non-negotiable principles of the project.

### R1: Configuration-Driven Tools

- **R1.1**: The system **must** adapt any command-line tool for use as a set of MCP tools based on declarative JSON configuration files.
- **R1.2**: All tool definitions **must** be stored in `.json` files within a `tools/` directory. The server discovers and loads these at runtime.
- **R1.3**: The system **must not** be recompiled to add, remove, or modify a tool. The server's source code must remain generic and tool-agnostic. All tool-specific logic is defined in the JSON configuration.

### R2: Sync-First Architecture (Updated 2025-01-09)

- **R2.1**: Operations **must** execute synchronously by default. When an AI invokes a tool, the server executes the command and returns the complete result in a single response.
- **R2.2**: This provides immediate feedback and simplifies the AI interaction model for most common development tasks.
- **R2.3**: Synchronous operations block until completion and return their final result directly. They do not use operation IDs or send completion notifications.
- **R2.4**: For long-running operations that should not block, tools can be marked as asynchronous using the `"force_synchronous": false` configuration (see R3.1).

### R3: Selective Asynchronous Override

- **R3.1**: Long-running, non-blocking operations (e.g., `cargo build`, `npm install`) **can** be marked as asynchronous in their JSON configuration (`"force_synchronous": false`).
- **R3.2**: Asynchronous operations **must** immediately return an `operation_id` and a `started` status, then execute the command in the background.
- **R3.3**: Upon completion of an asynchronous operation, the system **must** automatically push the final result (success or failure) to the AI client via an MCP progress notification.
- **R3.4**: Launching `ahma_mcp` with the `--async` flag **must** override all tool configuration defaults for that session, forcing every tool invocation to execute asynchronously.
- **R3.5**: Tool descriptions for async operations **must** explicitly guide the AI to continue with other tasks and not to wait, processing the result notification when it arrives.

### R4: Performance

- **R4.1**: The system **must** use a pre-warmed shell pool to minimize command startup latency, aiming for startup times of 5-20ms for commands in a known working directory.

### R5: JSON Schema and Validation

- **R5.1**: The system **must** implement comprehensive JSON schema validation for all tool configurations against the MCP Tool Definition Format (MTDF).
- **R5.2**: Validation **must** occur at server startup. Invalid tool configurations must be rejected and not loaded, with clear error messages logged.
- **R5.3**: The schema must support types (`string`, `boolean`, `integer`, `array`), required fields, and security validation for file paths (`"format": "path"`).

### R6: Modular Architecture

- **R6.1**: The project **must** be organized as a Cargo workspace with clearly separated concerns to improve maintainability and enable future extensions.
- **R6.2**: Core library functionality (tool execution, configuration, async orchestration, MCP service) **must** live in the `ahma_core` crate, which is protocol-agnostic and reusable.
- **R6.3**: The main MCP server logic **must** live in the `ahma_shell` binary crate, which depends on `ahma_core`.
- **R6.4**: Tool configuration validation logic **must** be implemented in the `ahma_validate` binary crate. This provides a fast, focused way to check tool definitions for correctness without starting the full server.
- **R6.5**: The MTDF JSON Schema generation logic **must** be implemented in the `generate_tool_schema` binary crate.
- **R6.6**: The core library **must** expose a clean public API that allows other crates (like future `ahma_web` or `ahma_okta` components) to leverage the tool execution engine without tight coupling.
- **R6.7**: This separation ensures that adding new interfaces (web, authentication) or changing the CLI does not require modifications to core business logic.

## 3. Tool Definition (MTDF Schema)

All tools are defined in `.json` files in the `tools/` directory. This is the MCP Tool Definition Format (MTDF).

### 3.1. Basic Structure

```json
{
  "command": "base_executable",
  "enabled": true,
  "timeout_seconds": 600,
  "subcommand": [
    {
      "name": "subcommand_name",
      "description": "What this subcommand does. Include async guidance if asynchronous: true.",
      "force_synchronous": true,
      "options": [
        {
          "name": "option_name",
          "type": "string",
          "description": "Description of the option.",
          "required": true
        }
      ],
      "positional_args": [
        {
          "name": "arg_name",
          "type": "string",
          "description": "Description of the positional argument.",
          "required": true
        }
      ]
    }
  ]
}
```

### 3.2. Key Fields

- `command`: The base command-line executable (e.g., `git`, `cargo`).
- `subcommand`: An array of subcommands exposed as individual MCP tools. The final tool name will be `{command}_{name}` (e.g., `git_commit`).
- `asynchronous`: `false` for sync (default), `true` for async.
- `options`: An array of command-line flags (e.g., `--release`).
- `positional_args`: An array of positional arguments.
- `format: "path"`: **CRITICAL**: Any option or argument that accepts a file path **must** include this for security validation.

### 3.3. Sequence Tools

Sequence tools allow for chaining multiple commands into a single, synchronous workflow.

## 4. Development Workflow

To ensure code quality, stability, and adherence to these requirements, all AI maintainers **must** follow this workflow.

### 4.1. Pre-Commit Quality Check

Before committing any changes, developers **must** run the comprehensive quality check tool:

```bash
ahma_mcp rust_quality_check
```

This command is a sequence tool that performs the following essential steps in order:

1. **`generate_tool_schema`**: Regenerates the MTDF JSON schema to ensure it is up-to-date with any changes in the core data structures.
2. **`ahma_validate`**: Validates all tool configurations against the latest schema.
3. **`cargo fmt`**: Formats all Rust code.
4. **`cargo clippy`**: Lints the code and automatically applies fixes.
5. **`cargo nextest run`**: Runs the entire test suite.
6. **`cargo build`**: Compiles the project.

Only if all these steps pass should the code be considered ready for commit. This process prevents regressions and ensures that the project remains in a consistently healthy state.

- **Requirement R7.1**: The system **must** support sequence tools to execute a series of predefined steps in order.
- **Requirement R7.2**: A sequence is defined by setting `"command": "sequence"` and providing a `sequence` array.
- **Requirement R7.3**: A configurable delay (`step_delay_ms`) **must** be supported between steps to prevent resource conflicts (e.g., `Cargo.lock` contention).

#### Example Sequence Tool

See .ahma/tools/rust_quality_check.json

## 4. Development & Testing Workflow

### 4.1. Server Restart and Testing

**R8.1**: Always use `ahma_mcp` running in VS Code to interactively test the server.

**R8.2**: To restart the server after code changes:

- Run `cargo build --release` (either via terminal or `ahma_mcp` MCP tool)
- Reload the VS Code window (Cmd+Shift+P → "Developer: Reload Window") to restart the MCP server
- Alternatively, kill the running `ahma_mcp` process and VS Code will restart it automatically
- The server reads tool configurations from `.ahma/tools/` on each startup

**R8.3**: Interactive Testing Process:

1. Make code or configuration changes
2. Run `cargo build --release` to trigger server restart
3. Test the modified tool immediately through the MCP interface
4. If a tool does not work correctly, fix it immediately and restart
5. Verify the fix works before proceeding

**R8.4**: Follow strict TDD principles:

- Write unit/integration tests for new functionality
- Use interactive testing to diagnose and verify fixes
- Never leave broken tools—fix and test immediately

### 4.2. AI Maintainer Workflow

When a new task is assigned:

1. The changes required will be described in an update to this `requirements.md` file.
2. Your primary task is to implement the changes described herein.
3. If the task involves adding or changing tool behavior, you **must** achieve this by editing the JSON files in the `.ahma/tools/` directory.
4. You **must not** modify the Rust source code to add tool-specific logic.
5. Follow the existing development principles: write tests for new functionality and ensure all code is formatted (`cargo fmt`) and free of linter warnings (`cargo clippy`).
6. After any code changes affecting tool execution, restart the server (`cargo build--release`) and test interactively.
7. **Before stopping work**, you **must** run the `ahma_mcp rust_quality_check` tool and verify that all checks pass. This sequence now includes schema generation and validation, providing a comprehensive pre-flight check. Do not mark work as complete until this quality check succeeds.

### 4.3. CRITICAL: Use ahma_mcp MCP Server for All Operations

**R8.5**: AI maintainers working in Cursor **MUST** use the `ahma_mcp` MCP server for ALL development operations. **DO NOT** use terminal commands via `run_terminal_cmd` tool.

**Why**: We dogfood our own project to rapidly identify and fix issues during development. This ensures the project works correctly in real-world usage and catches bugs immediately.

**How to use ahma_mcp MCP server in Cursor**:
- The `ahma_mcp` server is configured in Cursor's MCP settings and runs automatically
- AI assistants in Cursor have access to MCP tools exposed by ahma_mcp
- Use these MCP tools directly via the MCP protocol (they appear as available tools in Cursor)
- Tool naming convention: `{command}_{subcommand}` (e.g., `cargo_build`, `cargo_nextest_run`, `cargo_clippy`)

**Examples of correct usage**:
- ❌ WRONG: `run_terminal_cmd("cargo nextest run")`
- ✅ CORRECT: Call MCP tool `cargo_nextest_run` via Cursor's MCP interface
- ❌ WRONG: `run_terminal_cmd("cargo build --release")`  
- ✅ CORRECT: Call MCP tool `cargo_build` with `release: true` parameter
- ❌ WRONG: `run_terminal_cmd("cargo clippy --fix")`
- ✅ CORRECT: Call MCP tool `cargo_clippy` with `fix: true` parameter

**R8.6**: If an MCP tool is missing, broken, or doesn't work as expected, that is a **bug in ahma_mcp** that must be fixed. Document the issue and only work around it if absolutely necessary for urgent fixes.

**R8.7**: **Terminal Fallback for Broken ahma_mcp State**:
- If the ahma_mcp MCP server is not responding or returning errors, you **MAY** temporarily use `run_terminal_cmd` as a fallback
- When using terminal fallback, you **MUST**:
  1. Add a TODO task to fix the ahma_mcp errors that caused the fallback
  2. After making any code changes, run `cargo build --release` via terminal
  3. The mcp.json watch configuration will detect the binary change and restart the server
  4. After restart, **immediately** switch back to using MCP tools and verify they work
  5. If MCP tools still don't work, investigate and fix the root cause before proceeding
- This fallback is **temporary** - the goal is always to have ahma_mcp working so we can dogfood it

**R8.8**: **Restarting ahma_mcp Server**:
- After code changes: Run `cargo build --release` (via MCP tool or terminal fallback)
- The mcp.json configuration watches the binary and automatically restarts the server
- Verify restart by calling a simple MCP tool like `status` or `cargo_check`
- If server doesn't restart automatically, reload Cursor window (Cmd+Shift+P → "Developer: Reload Window")

**R8.9**: For AI sessions outside of Cursor (e.g., in other contexts), use the command-line interface: `ahma_mcp --tool_name <tool> --tool_args <args>` to invoke tools directly.

### 4.3. Copilot CLI Verification

- **R9.1**: Copilot LLMs **should** validate code and tool changes by invoking `ahma_mcp` directly from the command line using the `--tool_name` and `--tool_args` parameters. This keeps validation steps reproducible and scriptable during autonomous runs.
- **R9.2**: Always pass the MCP tool identifier to `--tool_name`, and supply the exact arguments that would normally be provided through the MCP interface via `--tool_args`.
- **R9.3**: Use the double-dash (`--`) separator within `--tool_args` to forward raw positional arguments exactly as the target CLI expects when necessary.
- **Example – rebuild after code changes**: `ahma_mcp --tool_name cargo_build --tool_args -- --release`
- **Example – run quality checks**: `ahma_mcp rust_quality_check`

## 5. Implementation Constraints and Architecture Decisions

This section documents critical implementation details discovered through analysis and testing.

### 5.1. Meta-Parameters

**R10.1**: Certain parameters are "meta-parameters" that control the execution environment but should not be passed as command-line arguments to tools:

- `working_directory`: Specifies where the command executes
- `execution_mode`: Controls sync vs async execution  
- `timeout_seconds`: Sets operation timeout

**R10.2**: The adapter layer **must** filter out meta-parameters when constructing command-line arguments.

**R10.3**: Sequence tools **must** preserve `working_directory` across steps but filter it from being passed as a CLI argument.

### 5.2. Sequence Tool Architecture

**R10.4**: There are two distinct types of sequence tools with different structural requirements:

#### Top-Level Sequences (Cross-Tool Orchestration)

- **R10.4.1**: Tools that orchestrate multiple different tools (e.g., `rust_quality_check` calling `cargo`, `ahma_validate`, etc.) **must** define their sequence at the top level of the tool configuration.
- **R10.4.2**: Structure: `{"command": "sequence", "sequence": [{...}], "step_delay_ms": 500}`
- **R10.4.3**: Each sequence step specifies `tool` and `subcommand` to invoke.
- **R10.4.4**: Handled by `handle_sequence_tool()` in `mcp_service.rs`.

#### Subcommand Sequences (Intra-Tool Workflows)

- **R10.4.5**: Subcommands that need to execute multiple steps within the same tool context **may** define a sequence at the subcommand level.
- **R10.4.6**: Structure: `{"subcommand": [{"name": "x", "sequence": [{...}]}]}`
- **R10.4.7**: Used for complex workflows within a single tool.
- **R10.4.8**: Handled by `handle_subcommand_sequence()` in `mcp_service.rs`.

**R10.5**: The choice between top-level and subcommand-level sequences is architectural, not configuration preference. Cross-tool orchestration requires top-level sequences.

### 5.3. Dependency Management

**R11.1**: The project uses minimal, high-quality dependencies chosen for:

- Reliability and maintenance
- Performance
- Minimal transitive dependencies
- Clear licensing

**R11.2**: Current core dependencies:

- `rmcp`: MCP protocol implementation
- `tokio`: Async runtime
- `serde`/`serde_json`: Serialization
- `anyhow`: Error handling
- `tracing`: Structured logging

**R11.3**: Before adding a new dependency, justify it in the PR description and consider alternatives.

### 5.4. Error Handling Patterns

**R12.1**: Use `anyhow::Result` for internal error propagation.

**R12.2**: Convert to `McpError` at the MCP service boundary for client communication.

**R12.3**: Include actionable context in error messages (e.g., "Install with `cargo install cargo-nextest`").

**R12.4**: Log errors at appropriate levels:

- `error!`: Operation failures affecting user workflows
- `warn!`: Recoverable issues or deprecated usage
- `info!`: Normal operation milestones
- `debug!`: Detailed troubleshooting information

### 5.5. Testing Philosophy

**R13.1**: All new functionality **must** have tests.

**R13.2**: Test organization:

- Unit tests: In-module `#[cfg(test)]` blocks or `tests/` directory
- Integration tests: Cross-module workflows
- Regression tests: Bug fixes must include a test that would have caught the bug

**R13.3**: Tests should be:

- **Fast**: Most tests complete in <100ms
- **Isolated**: No shared mutable state
- **Deterministic**: Same input always produces same output
- **Documented**: Test names describe what they verify

**R13.4**: The `rust_quality_check` tool runs the full test suite and must pass before committing.

### R14: HTTP MCP Client and OAuth Authentication

- **R14.1**: `ahma_mcp` **must** be able to act as an MCP client for HTTP-based MCP servers, enabling it to connect to services like the Atlassian MCP server.
- **R14.2**: This functionality **is** implemented in the `ahma_http_mcp_client` crate within the Cargo workspace to maintain modularity as per principle R6.
- **R14.3**: The client **must** support the OAuth 2.0 authorization code flow with PKCE for user authentication.
- **R14.4**: When authentication is required, the system **must** provide the user with a URL to open in their web browser. The system attempts to open the browser automatically using the `webbrowser` crate; if that fails, it displays the link for the user to copy.
- **R14.5**: After successful user authentication in the browser, the client **must** handle the OAuth callback on `http://localhost:8080`, retrieve the authorization code, and exchange it for an access token and a refresh token using the `oauth2` crate. These tokens are stored in the system's temporary directory as `mcp_http_token.json`.
- **R14.6**: All subsequent requests to the MCP server **must** be authenticated using the stored access token via Bearer authentication. Token refresh logic is planned but not yet implemented.
- **R14.7**: The `mcp.json` configuration file **supports** definitions for HTTP-based MCP servers with the following structure:
  ```json
  {
    "servers": {
      "server_name": {
        "type": "http",
        "url": "https://api.example.com/mcp",
        "client_id": "your_client_id",
        "client_secret": "your_client_secret"
      }
    }
  }
  ```
- **R14.8**: The HTTP transport **implements** the `rmcp::transport::Transport<RoleClient>` trait, providing bidirectional communication:
  - **Sending**: HTTP POST requests with Bearer authentication for outgoing messages
  - **Receiving**: Server-Sent Events (SSE) for incoming messages from the server (background task)
- **R14.9**: **Current Status**: The `HttpMcpTransport` is fully implemented and compiles successfully. Integration with the main server binary is pending completion of rmcp 0.9.0 client API documentation and examples.

## 6. Known Limitations and Future Work

### 6.1. Current Limitations

- Nested subcommands beyond 2 levels are not tested extensively
- Schema validation is comprehensive but error messages could be more helpful
- No built-in retry logic for transient failures
- Limited Windows testing (primarily developed/tested on macOS/Linux)

### 6.2. Planned Improvements

See `ARCHITECTURE_UPGRADE_PLAN.md` for detailed roadmap including:

- Enhanced type safety with newtype wrappers
- Improved error messages with suggestions
- Better tool discoverability and categorization
- Performance optimizations
- Enhanced documentation

## 7. Version History

- **v0.5.0** (2025-11-18):
  - Added requirement for HTTP MCP Client with OAuth support
  - Implemented `ahma_http_mcp_client` crate with full OAuth 2.0 + PKCE flow
  - Implemented `HttpMcpTransport` using rmcp 0.9.0 `Transport` trait
  - Updated to rmcp 0.9.0 (breaking changes: added `meta` field to `Tool` struct)
  - Updated to oauth2 5.0.0 (breaking changes: new client builder API)
  - Added `mcp.json` configuration support for HTTP-based MCP servers
  - HTTP MCP client integration pending rmcp 0.9.0 client API examples
- **v0.4.0** (2025-11-16):
  - Fixed sequence tool architecture (top-level vs subcommand-level)
  - Fixed meta-parameter handling in command construction
  - Updated tests to reflect correct sequence structure
  - Added comprehensive architecture documentation
- **v0.3.x**: Initial async/sync hybrid implementation
- **v0.2.x**: Basic MCP server functionality
- **v0.1.x**: Prototype

---

**Last Updated**: 2025-11-18
**Status**: Living Document - Update with every architectural decision or significant change
