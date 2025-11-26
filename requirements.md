# Ahma MCP Requirements

This document is the single source of truth for the `ahma_mcp` project. It outlines the core requirements, architecture, and principles that an AI maintainer must follow. All new development tasks will be reflected as changes in this document.

## 1. Core Mission

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for use by AI agents. Its purpose is to provide a consistent, powerful, and non-blocking bridge between AI and the vast ecosystem of command-line utilities.

## 2. Core Principles & Requirements

These are the non-negotiable principles of the project.

### R0: Runtime Terminology Alignment

- **R0.1**: The running MCP server that Cursor/VS Code connects to is now named **"Ahama"** in `mcp.json` (e.g., `servers.Ahama`). This naming is reserved for conversations about the live MCP experience.
- **R0.2**: The Git repository, source code, and compiled binary remain `ahma_mcp`. When referencing build steps, code changes, or CLI invocations (e.g., `ahma_mcp cargo_qualitycheck`), always use `ahma_mcp`.
- **R0.3**: All written guidance and AI conversations **must** explicitly distinguish whether they are referring to `Ahama` (the running MCP service) or `ahma_mcp` (the project used to build it) to avoid ambiguity.
- **R0.4**: Any instruction that restarts the MCP experience **must** frame it as: "build the `ahma_mcp` binary, which restarts the `Ahama` entry defined in `mcp.json`".

### R1: Configuration-Driven Tools

- **R1.1**: The system **must** adapt any command-line tool for use as a set of MCP tools based on declarative JSON configuration files.
- **R1.2**: All tool definitions **must** be stored in `.json` files within a `tools/` directory. The server discovers and loads these at runtime.
- **R1.3**: The system **must not** be recompiled to add, remove, or modify a tool. The server's source code must remain generic and tool-agnostic. All tool-specific logic is defined in the JSON configuration.

### R2: Sync-First Architecture (Updated 2025-01-09)

- **R2.1**: Operations **must** execute synchronously by default. When an AI invokes a tool, the server executes the command and returns the complete result in a single response.
- **R2.2**: This provides immediate feedback and simplifies the AI interaction model for most common development tasks.
- **R2.3**: Synchronous operations block until completion and return their final result directly. They do not use operation IDs or send completion notifications.
- **R2.4**: For long-running operations that should not block, tools can be marked as asynchronous using the `"force_synchronous": false` configuration (see R3.1).
- **R2.5**: Commands that modify project configuration files (e.g., `Cargo.toml`, `package.json`) **must** use `"force_synchronous": true` to prevent race conditions and ensure the AI receives confirmation before proceeding with dependent operations. Examples include `cargo add`, `cargo upgrade`, `npm install --save`.
- **R2.6**: **Inheritance**: `force_synchronous` can be set at the tool level or subcommand level. Subcommand-level settings override tool-level settings. If a subcommand does not specify `force_synchronous`, it inherits from the tool level. This allows setting a default for an entire tool while overriding specific subcommands as needed.

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
- **R6.6**: Project-specific quality assurance tools (e.g., `ahma_quality_check`) **may** include schema generation and validation steps as part of their workflow, while generic tools (e.g., `cargo qualitycheck` subcommand) **must** remain reusable across projects.
- **R6.7**: The core library **must** expose a clean public API that allows other crates (like future `ahma_web` or `ahma_okta` components) to leverage the tool execution engine without tight coupling.
- **R6.8**: This separation ensures that adding new interfaces (web, authentication) or changing the CLI does not require modifications to core business logic.
- **R6.9**: The root `Cargo.toml` **must** define `default-members = ["ahma_shell"]` so that `cargo run` executes the main MCP server binary by default.
- **R6.10**: The `ahma_list_tools` binary crate **must** provide a CLI utility to dump all MCP tool information from an MCP server (stdio or HTTP mode) to the terminal, useful for tests and development verification.

### R7: Security First (Added 2025-01-27)

- **R7.1**: The system **must** enforce strict path validation for all file system operations.
- **R7.2**: Access **must** be restricted to the current working directory and its subdirectories.
- **R7.3**: Any attempt to access paths outside the workspace (e.g., `/`, `../`) **must** be rejected immediately.
- **R7.4**: Command arguments **must** be validated to prevent shell injection and unauthorized path access.

### R8: HTTP Bridge (Added 2025-11-23)

- **R8.1**: The system **must** support an HTTP bridge mode (`ahma_mcp --mode http`) that exposes the MCP server over HTTP/SSE.
- **R8.2**: The bridge **must** support Server-Sent Events (SSE) at `/sse` for server-to-client notifications and events.
- **R8.3**: The bridge **must** support JSON-RPC requests via POST at `/mcp`.
- **R8.4**: The bridge **must** handle concurrent requests by matching JSON-RPC IDs.
- **R8.5**: The bridge **must** be robust against underlying process crashes, automatically restarting the `ahma_mcp` stdio process if it terminates.

## 3. Tool Definition (MTDF Schema)

All tools are defined in `.json` files in the `tools/` directory. This is the MCP Tool Definition Format (MTDF).

### 3.1. Basic Structure

```json
{
  "name": "tool_name",
  "description": "What this tool does.",
  "command": "base_executable",
  "enabled": true,
  "timeout_seconds": 600,
  "force_synchronous": true,
  "subcommand": [
    {
      "name": "subcommand_name",
      "description": "What this subcommand does. Include async guidance if force_synchronous: false.",
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
    },
    {
      "name": "async_subcommand",
      "description": "This subcommand runs async. Include async guidance here.",
      "force_synchronous": false
    }
  ]
}
```

### 3.2. Key Fields

- `command`: The base command-line executable (e.g., `git`, `cargo`).
- `subcommand`: An array of subcommands exposed as individual MCP tools. The final tool name will be `{command}_{name}` (e.g., `git_commit`).
- `force_synchronous`: Can be set at tool level or subcommand level. Subcommand-level overrides tool-level. Set to `true` for commands that must complete before dependent operations (e.g., `cargo add`). Set to `false` for long-running commands that can run in the background (e.g., `cargo build`).
- `options`: An array of command-line flags (e.g., `--release`).
- `positional_args`: An array of positional arguments.
- `format: "path"`: **CRITICAL**: Any option or argument that accepts a file path **must** include this for security validation.

### 3.3. Sequence Tools

Sequence tools allow for chaining multiple commands into a single, synchronous workflow.

## 4. Development Workflow

To ensure code quality, stability, and adherence to these requirements, all AI maintainers **must** follow this workflow.

### 4.1. Pre-Commit Quality Check

Before committing any changes, developers **must** run the comprehensive quality check. **IMPORTANT**: Ahama is already running as an MCP server in your IDE - use it directly via MCP tools, not via terminal commands.

**How to run quality checks via Ahama MCP:**

The Ahama MCP server (defined in your IDE's `mcp.json`) is already running and provides access to all cargo tools. Talk to the server to see what tools are available, then use these MCP tools directly. For example, use the `sandboxed_shell` tool to run any command in a shell with sandboxing that prevents access outside of the current working directory.

**For the ahma_mcp project:** Use the `ahma_quality_check` tool, which includes schema generation and tool validation specific to this project.

**For generic Rust projects:** Use the `cargo_qualitycheck` tool (a subcommand within cargo.json), which provides standard Rust quality checks (format, lint, test, build) without project-specific steps.

Only if all these steps pass should the code be considered ready for commit. This process prevents regressions and ensures that the project remains in a consistently healthy state.

- **Requirement R7.1**: The system **must** support sequence tools to execute a series of predefined steps in order.
- **Requirement R7.2**: A sequence is defined by setting `"command": "sequence"` and providing a `sequence` array.
- **Requirement R7.3**: A configurable delay (`step_delay_ms`) **must** be supported between steps to prevent resource conflicts (e.g., `Cargo.lock` contention).

#### Example Sequence Tool

See the `qualitycheck` subcommand in `.ahma/tools/cargo.json` for a subcommand-level sequence example, or `.ahma/tools/ahma_quality_check.json` for a top-level sequence tool example.

## 4. Development & Testing Workflow

### 4.2. Server Restart and Testing

**R8.1**: Always use **Ahama** - the MCP server already running in your IDE (VS Code/Cursor) as defined in `mcp.json`. The server is backed by the `ahma_mcp` binary and provides all tools via the MCP protocol.

**R8.2**: To restart the server after code changes:

- Use Ahama's `cargo` tool with `build` subcommand and `release: true` option via MCP
- Reload the IDE window (Cmd+Shift+P → "Developer: Reload Window") to restart the MCP server
- Alternatively, kill the running `ahma_mcp` process and the IDE will restart it automatically
- The server reads tool configurations from `.ahma/tools/` on each startup

**R8.3**: Interactive Testing Process (Ahama runtime):

1. Make code or configuration changes
2. Use Ahama's `cargo build` MCP tool (with `release: true`) to trigger server restart
3. Test the modified tool immediately through the MCP interface
4. If a tool does not work correctly, fix it immediately and restart
5. Verify the fix works before proceeding
6. If a tool does not work correctly, fix it immediately and restart
7. Verify the fix works before proceeding

**R8.4**: Follow strict TDD principles:

- Write unit/integration tests for new functionality
- Use interactive testing to diagnose and verify fixes
- Never leave broken tools—fix and test immediately

### 4.3. AI Maintainer Workflow

When a new task is assigned:

1. The changes required will be described in an update to this `requirements.md` file.
2. Your primary task is to implement the changes described herein.
3. If the task involves adding or changing tool behavior, you **must** achieve this by editing the JSON files in the `.ahma/tools/` directory.
4. You **must not** modify the Rust source code to add tool-specific logic.
5. Follow the existing development principles: write tests for new functionality and ensure all code is formatted and free of linter warnings.
6. After any code changes affecting tool execution, restart the server using Ahama's `cargo build` MCP tool (with `release: true`) and test interactively.
7. **Before stopping work**, you **must** use Ahama's `cargo_qualitycheck` sequence tool (or run individual quality checks via Ahama's `cargo` MCP tools) and verify that all checks pass. This provides a comprehensive pre-flight check. Do not mark work as complete until this quality check succeeds.

### 4.4. CRITICAL: Always Use Ahama MCP Server (Already Running in Your IDE)

**R8.5**: AI maintainers working in Cursor/VS Code **MUST** use the **Ahama** MCP server for ALL development operations. Ahama is **already running** in your IDE as configured in `mcp.json`. **DO NOT** use terminal commands via `run_in_terminal` tool.

**Why**: We dogfood our own project to rapidly identify and fix issues during development. This ensures the project works correctly in real-world usage and catches bugs immediately.

### 4.4.1 CRITICAL: Use `sandboxed_shell` MCP Tool Instead of Terminal

**R8.5.1**: **NEVER** use `run_in_terminal` or similar IDE terminal tools. **ALWAYS** use the `sandboxed_shell` MCP tool instead.

**Why this matters for LLM efficiency**:

- Using `run_in_terminal` triggers GUI "Allow" permission dialogs that slow down development
- Each terminal command requires user approval, breaking your workflow
- The `sandboxed_shell` MCP tool runs immediately without GUI interruptions
- You will complete tasks **much faster** using MCP tools

**Security benefits of `sandboxed_shell`**:

- All file paths are validated and constrained to the current working directory
- Prevents accidental access to files outside the project (e.g., `/`, `~/`, `../`)
- Commands are executed in a controlled sandbox environment
- Path traversal attacks are blocked automatically

**Examples**:

- ❌ WRONG: `run_in_terminal("sed -i '' 's/old/new/g' file.rs")`
- ✅ CORRECT: Call MCP tool `sandboxed_shell` with `{"command": "sed -i '' 's/old/new/g' file.rs"}`
- ❌ WRONG: `run_in_terminal("grep -r 'pattern' src/")`
- ✅ CORRECT: Call MCP tool `sandboxed_shell` with `{"command": "grep -r 'pattern' src/"}`
- ❌ WRONG: `run_in_terminal("find . -name '*.rs' -exec wc -l {} +")`
- ✅ CORRECT: Call MCP tool `sandboxed_shell` with `{"command": "find . -name '*.rs' -exec wc -l {} +"}`

**R8.5.2**: The `sandboxed_shell` tool supports all standard shell features: pipes, redirects, variables, command substitution, etc. There is no functionality loss compared to terminal access.

**How Ahama works in your IDE**:

- The `ahma_mcp` binary is already running as an MCP server named "Ahama" (configured in your IDE's `mcp.json`)
- AI assistants in Cursor/VS Code have direct access to MCP tools exposed by Ahama
- Use these MCP tools directly via the MCP protocol (they appear as available tools)
- Tool naming convention: `{command}_{subcommand}` (e.g., `cargo_build`, `cargo_nextest_run`, `cargo_clippy`)
- Or use the tool name with subcommand parameter: `cargo` with `{"subcommand": "build", "release": true}`

**Examples of correct usage**:

- ❌ WRONG: `run_terminal_cmd("cargo nextest run")`
- ✅ CORRECT: Call MCP tool `cargo` with `{"subcommand": "nextest", "nextest_subcommand": "run"}` via Ahama
- ❌ WRONG: `run_terminal_cmd("cargo build --release")`
- ✅ CORRECT: Call MCP tool `cargo` with `{"subcommand": "build", "release": true}` via Ahama
- ❌ WRONG: `run_terminal_cmd("cargo clippy --fix")`
- ✅ CORRECT: Call MCP tool `cargo` with `{"subcommand": "clippy", "fix": true, "allow-dirty": true}` via Ahama
- ❌ WRONG: `run_terminal_cmd("cargo fmt")`
- ✅ CORRECT: Call MCP tool `cargo` with `{"subcommand": "fmt"}` via Ahama

**R8.6**: If an MCP tool is missing, broken, or doesn't work as expected, that is a **bug in ahma_mcp** that must be fixed. Document the issue and only work around it if absolutely necessary for urgent fixes.

**R8.7**: **Terminal Fallback for Broken Ahama State** (RARE - should almost never happen):

- If the Ahama MCP server is completely non-responsive or returning errors for all tools, you **MAY** temporarily use `run_terminal_cmd` as a fallback
- When using terminal fallback, you **MUST** do all of the following:

  1. Add a TODO task to fix the ahma_mcp errors that caused the fallback
  2. After making any code changes, use terminal to run `cargo build --release`
  3. The mcp.json watch configuration will detect the binary change and restart the server
  4. After restart, **immediately** switch back to using Ahama MCP tools and verify they respond
  5. If MCP tools still don't work, investigate and fix the root cause before proceeding

- This fallback is **temporary and rare** - the goal is always to have Ahama working so we can dogfood it

**R8.8**: **Restarting Ahama** (after code changes):

- Use Ahama's `cargo` MCP tool with `{"subcommand": "build", "release": true}`
- The mcp.json configuration watches the binary and automatically restarts the server
- Verify restart by calling a simple MCP tool like `status` or `cargo` with `{"subcommand": "check"}` via Ahama
- If server doesn't restart automatically, reload the IDE window (Cmd+Shift+P → "Developer: Reload Window")

**R8.9**: For CLI testing outside the IDE (e.g., in shell scripts), use: `ahma_mcp --tool_name <tool> --tool_args <args>` to invoke tools directly.

**R8.10**: Launching the binary without an explicit `--mode` or `--tool_name` is intentionally rejected, and **interactive terminals are blocked even when `--mode stdio` is provided**. The stdio server only runs when stdin is **not** a TTY (i.e., when a real MCP client spawns the process and attaches pipes). Local testing should therefore happen through the Ahama MCP server already running in your IDE. To execute a single tool outside the IDE, run `ahma_mcp --tool_name <tool> ...`.

### R11: HTTP MCP Client Token Storage (Added 2025-11-25)

- **R11.1**: The HTTP MCP client stores OAuth tokens in the OS temporary directory by default but **must** respect the `AHMA_HTTP_CLIENT_TOKEN_PATH` environment variable when it is set. This keeps automated tests and multi-user environments from trampling each other's credentials.
- **R11.2**: Tests that override the token location **must** direct it to a temporary directory (e.g., via the `tempfile` crate) to ensure automatic cleanup and avoid polluting the repository.

### 4.5. Copilot CLI Verification

- **R9.1**: For command-line verification outside the IDE, you can invoke `ahma_mcp` directly using the `--tool_name` and `--tool_args` parameters. This keeps validation steps reproducible and scriptable.
- **R9.2**: Always pass the MCP tool identifier to `--tool_name`, and supply the exact arguments that would normally be provided through the MCP interface via `--tool_args`.
- **R9.3**: Use the double-dash (`--`) separator within `--tool_args` to forward raw positional arguments exactly as the target CLI expects when necessary.
- **Example – rebuild after code changes**: `ahma_mcp --tool_name cargo --tool_args '{"subcommand": "build", "release": true}'`
- **Example – run quality checks**: `ahma_mcp --tool_name cargo_qualitycheck`

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

- **R10.4.1**: Tools that orchestrate multiple different tools (e.g., `ahma_quality_check` calling `cargo run` to generate schemas, `cargo fmt`, `cargo clippy`, etc.) **must** define their sequence at the top level of the tool configuration.
- **R10.4.2**: Structure: `{"command": "sequence", "sequence": [{...}], "step_delay_ms": 500}`
- **R10.4.3**: Each sequence step specifies `tool` and `subcommand` to invoke.
- **R10.4.4**: Handled by `handle_sequence_tool()` in `mcp_service.rs`.
- **R10.4.5**: Sequence tools **must** be generic and reusable across projects. Project-specific validation or generation steps belong in dedicated project-specific sequence tools (like `ahma_quality_check`), not in generic quality check tools.

#### Subcommand Sequences (Intra-Tool Workflows)

- **R10.4.6**: Subcommands that need to execute multiple steps within the same tool context **may** define a sequence at the subcommand level (e.g., `cargo qualitycheck` subcommand).
- **R10.4.7**: Structure: `{"subcommand": [{"name": "qualitycheck", "sequence": [{...}], "step_delay_ms": 500}]}`
- **R10.4.8**: Used for complex workflows within a single tool, invoked as `tool_subcommand` (e.g., `cargo_qualitycheck`).
- **R10.4.9**: Handled by `handle_subcommand_sequence()` in `mcp_service.rs`.
- **R10.4.10**: **CRITICAL**: Subcommand names **must not** contain underscores, as underscores are used as hierarchical separators in the tool invocation system. For example, `cargo_qualitycheck` maps to the `cargo` tool's `qualitycheck` subcommand. Using `quality_check` would cause parsing issues.

**R10.5**: The choice between top-level and subcommand-level sequences is architectural, not configuration preference. Cross-tool orchestration requires top-level sequences. Intra-tool workflows use subcommand sequences.

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

### 5.4. Async I/O Hygiene (Added 2025-11-26)

**R16.1**: Blocking I/O calls from `std::fs` and `std::io` **must not** be used within async functions. Use `tokio::fs` and `tokio::io` equivalents instead to avoid blocking the async runtime.

**R16.2**: Functions performing file I/O **should** be async and use Tokio's async file operations:

- Use `tokio::fs::read_to_string` instead of `std::fs::read_to_string`
- Use `tokio::fs::write` instead of `std::fs::write`
- Use `tokio::fs::create_dir_all` instead of `std::fs::create_dir_all`
- Use `tokio::fs::read_dir` instead of `std::fs::read_dir`

**R16.3**: `tokio::task::spawn_blocking` **should** be avoided when the underlying operation can be reasonably converted to use async I/O. Reserve `spawn_blocking` for:

- Third-party libraries that only offer synchronous APIs
- CPU-bound computation (not I/O-bound operations)
- Operations where converting to async would require significant refactoring with minimal benefit

**R16.4**: Test code is exempt from this requirement. Tests may use `std::fs` directly since test execution is not performance-critical and test code typically runs in `#[tokio::test]` contexts where brief blocking is acceptable.

### 5.5. Error Handling Patterns

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

**R13.4**: The `cargo_qualitycheck` tool runs the full test suite and must pass before committing.

**R13.5**: Test File Isolation (CRITICAL):

- **ALL tests MUST use temporary directories** via the `tempfile` crate to prevent repository pollution
- **NEVER** create test files or directories directly in the repository structure
- Use `tempfile::tempdir()` or `tempfile::TempDir::new()` to create isolated test workspaces
- The `TempDir` type automatically cleans up on drop, ensuring no test artifacts remain
- Common patterns:

  ```rust
  use tempfile::tempdir;  // or use tempfile::TempDir;

  let temp_dir = tempdir().unwrap();  // Auto-cleanup on drop
  let client = new_client_in_dir(Some(".ahma/tools"), &[], temp_dir.path()).await.unwrap();

  // Create test files/directories within temp_dir.path()
  let test_file = temp_dir.path().join("test.txt");
  fs::write(&test_file, "test content").unwrap();
  ```

- For tests requiring complex project structures, use helper functions like `create_full_rust_test_project()` which return `TempDir` instances
- Test directories are automatically removed when the `TempDir` value goes out of scope
- This ensures tests can run in parallel without conflicts and leaves no artifacts in the repository

**R13.6**: CLI Binary Integration Testing:

- All CLI binaries (`ahma_mcp`, `ahma_validate`, `ahma_list_tools`, `generate_tool_schema`) **must** have integration tests
- Integration tests are located in `ahma_core/tests/cli_binary_integration_test.rs`
- Each binary should have tests for: `--help`, `--version`, and basic functionality
- Tests invoke binaries as external processes using `std::process::Command`
- These tests verify end-to-end CLI functionality that unit tests cannot cover
- Note: Binary coverage is not tracked by `cargo llvm-cov` since binaries run as subprocesses

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
        "atlassian_client_id": "your_client_id",
        "atlassian_client_secret": "your_client_secret"
      }
    }
  }
  ```

- **R14.8**: The HTTP transport **implements** the `rmcp::transport::Transport<RoleClient>` trait, providing bidirectional communication:
  - **Sending**: HTTP POST requests with Bearer authentication for outgoing messages
  - **Receiving**: Server-Sent Events (SSE) for incoming messages from the server (background task)
- **R14.9**: **Current Status**: The `HttpMcpTransport` is fully implemented and compiles successfully. Integration with the main server binary is pending completion of rmcp 0.9.0 client API documentation and examples.

### R15: Unified Shell Output (Added 2025-11-24)

- **R15.1**: All shell commands launched through `tokio::process::Command` **must** redirect stderr to stdout using `2>&1` so AI clients receive a single, chronologically ordered stream of output.
- **R15.2**: The adapter layer **must** automatically append `2>&1` to any shell script executed via `sh`, `bash`, or `zsh` `-c` invocations before the command is run.
- **R15.3**: Tests **must** cover this behavior to prevent regressions when new shell entry points are added.

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
