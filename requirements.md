# Ahma MCP Requirements

Technical specification for the `ahma_mcp` project. For AI development instructions, see [AGENTS.md](AGENTS.md).

---

## 1. Core Mission

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for use by AI agents. Its purpose is to provide a consistent, powerful, and non-blocking bridge between AI and the vast ecosystem of command-line utilities.

## 2. Core Principles & Requirements

These are the non-negotiable principles of the project.

### R0: Runtime Terminology Alignment

- **R0.1**: The running MCP server that Cursor/VS Code connects to is now named **"Ahama"** in `mcp.json` (e.g., `servers.Ahama`). This naming is reserved for conversations about the live MCP experience.
- **R0.2**: The Git repository, source code, and compiled binary remain `ahma_mcp`. When referencing build steps, code changes, or CLI invocations (e.g., `ahma_mcp --mode http --http-port 3000`), always use `ahma_mcp`.
- **R0.3**: All written guidance and AI conversations **must** explicitly distinguish whether they are referring to `Ahama` (the running MCP service) or `ahma_mcp` (the project used to build it) to avoid ambiguity.
- **R0.4**: Any instruction that restarts the MCP experience **must** frame it as: "build the `ahma_mcp` binary, which restarts the `Ahama` entry defined in `mcp.json`".

### R1: Configuration-Driven Tools

- **R1.1**: The system **must** adapt any command-line tool for use as a set of MCP tools based on declarative JSON configuration files.
- **R1.2**: All tool definitions **must** be stored in `.json` files within a `tools/` directory. The server discovers and loads these at runtime.
- **R1.3**: The system **must not** be recompiled to add, remove, or modify a tool. The server's source code must remain generic and tool-agnostic. All tool-specific logic is defined in the JSON configuration.
- **R1.4**: **Hot-Reloading**: The system **must** watch the `tools/` directory for changes to `.json` configuration files. When a file is added, modified, or removed, the system **must** automatically reload the tool definitions and send a `notifications/tools/list_changed` notification to connected clients to inform them of the update. This allows for rapid iteration on tool definitions without restarting the server.

### R2: Async-First Architecture (Updated 2025-11-29)

- **R2.1**: Operations **must** execute asynchronously by default. When an AI invokes a tool, the server immediately returns an operation ID and executes the command in the background.
- **R2.2**: This allows the AI to continue productive work while operations complete, maximizing efficiency for development workflows.
- **R2.3**: Asynchronous operations return an `operation_id` immediately and push results via MCP progress notifications when complete.
- **R2.4**: For operations that must complete before the AI proceeds, tools can be marked as synchronous using the `"synchronous": true` configuration (see R3.1).
- **R2.5**: Commands that modify project configuration files (e.g., `Cargo.toml`, `package.json`) **must** use `"synchronous": true` to prevent race conditions and ensure the AI receives confirmation before proceeding with dependent operations. Examples include `cargo add`, `cargo upgrade`, `npm install --save`.
- **R2.6**: **Inheritance**: `synchronous` can be set at the tool level or subcommand level. Subcommand-level settings override tool-level settings. If a subcommand does not specify `synchronous`, it inherits from the tool level (if set), otherwise defaults to async. This allows setting a default for an entire tool while overriding specific subcommands as needed.

### R3: Selective Synchronous Override

- **R3.1**: Operations that must complete before the AI proceeds (e.g., `cargo add`, `cargo upgrade`) **can** be marked as synchronous in their JSON configuration (`"synchronous": true`).
- **R3.2**: Asynchronous operations (the default) **must** immediately return an `operation_id` and a `started` status, then execute the command in the background.
- **R3.3**: Upon completion of an asynchronous operation, the system **must** automatically push the final result (success or failure) to the AI client via an MCP progress notification.
- **R3.4**: Launching `ahma_mcp` with the `--sync` flag **must** override all tool configuration defaults for that session, forcing every tool invocation to execute synchronously.
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
- **R6.3**: The main MCP server binary (`ahma_mcp`) **must** be defined in the `ahma_core` crate alongside the core library functionality.
- **R6.4**: Tool configuration validation logic **must** be implemented in the `ahma_validate` binary crate. This provides a fast, focused way to check tool definitions for correctness without starting the full server.
- **R6.5**: The MTDF JSON Schema generation logic **must** be implemented in the `generate_tool_schema` binary crate.
- **R6.6**: Project-specific quality assurance workflows **should** be expressed as shell command pipelines executed via the built-in `sandboxed_shell` tool (preferred). Tool configs that are marked `"enabled": false` are considered deprecated in this repo.
- **R6.7**: The core library **must** expose a clean public API that allows other crates (like future `ahma_web` or `ahma_okta` components) to leverage the tool execution engine without tight coupling.
- **R6.8**: This separation ensures that adding new interfaces (web, authentication) or changing the CLI does not require modifications to core business logic.
- **R6.9**: The root `Cargo.toml` **must** include `ahma_core` in `default-members` so that `cargo run` executes the main `ahma_mcp` binary by default.
- **R6.10**: The `ahma_list_tools` binary crate **must** provide a CLI utility to dump all MCP tool information from an MCP server (stdio or HTTP mode) to the terminal, useful for tests and development verification.

### R7: Security First - Kernel-Enforced Sandboxing (Updated 2025-11-26)

The sandbox scope defines the directory boundary within which all file system operations are permitted. The AI can freely read, write, and execute within the sandbox scope but has **zero access** outside it. This is enforced at the kernel level, not by parsing command strings.

#### R7.1: Sandbox Scope Definition

- **R7.1.1**: The sandbox scope **must** be set securely at server/session initialization and **cannot** be changed during the session.
- **R7.1.2**: For **stdio mode**, the sandbox scope defaults to the current working directory (`cwd`) when the stdio server process is started. This works correctly because the IDE (VS Code/Cursor) sets `cwd` in `mcp.json` to `${workspaceFolder}`.
- **R7.1.3**: For **HTTP mode**, the sandbox scope **must** be set once at the start of the HTTP session. The following mechanisms are supported (in order of precedence):
  1. `--sandbox-scope <path>` command-line parameter
  2. `AHMA_SANDBOX_SCOPE` environment variable
  3. The current working directory when the HTTP server starts
- **R7.1.4**: A command-line parameter (`--sandbox-scope <path>`) **must** be supported to override the sandbox scope for both stdio and HTTP modes. When provided, this parameter takes precedence over defaults.
- **R7.1.5**: The `working_directory` parameter in tool calls is no longer used to define the sandbox scope. The LLM **cannot** pass an insecure working directory that escapes the sandbox.
- **R7.1.6**: The `AHMA_SANDBOX_SCOPE` environment variable **must** be supported as an alternative to the `--sandbox-scope` CLI parameter. This enables configuration via shell profiles or systemd units without modifying command lines.
- **R7.1.7**: **HTTP mode single-sandbox limitation**: In HTTP bridge mode, one `ahma_mcp` subprocess handles all connections. All clients share the same sandbox scope set at server startup. For per-project isolation, run separate HTTP server instances with different sandbox scopes. Future versions may support per-session sandbox isolation (see `docs/session-isolation.md`).
- **R7.1.8**: HTTP mode is intended for **local development only**. The sandbox scope is trusted from the first connection and cannot be changed. Do not expose the HTTP server to untrusted networks.

#### R7.2: Kernel-Level Sandbox Enforcement (Linux)

- **R7.2.1**: On Linux, the system **must** use **Landlock** (kernel 5.13+) for kernel-level file system sandboxing.
- **R7.2.2**: The [`landlock` crate](https://crates.io/crates/landlock) **should** be used for Rust integration.
- **R7.2.3**: Landlock rules **must** restrict file system access to only the sandbox scope directory and its subdirectories.
- **R7.2.4**: If Landlock is not available (older kernel), the server **must** refuse to start and display clear instructions for how to enable it or upgrade the kernel.

#### R7.3: Kernel-Level Sandbox Enforcement (macOS)

- **R7.3.1**: On macOS, the system **must** use **sandbox-exec** with **Seatbelt profiles** (Apple's built-in sandboxing) for kernel-level file system sandboxing.
- **R7.3.2**: `sandbox-exec` is built into macOS and does not require installation. If not available (should not happen on any modern macOS), the server **must** refuse to start.
- **R7.3.3**: Seatbelt profiles **must** be written in Apple's Sandbox Profile Language (SBPL).
- **R7.3.4**: **CRITICAL SBPL Syntax**: Each `(allow ...)` statement with path filters **must** have all filters on the **same line** or use separate statements. Multi-line indented subpaths cause `sandbox-exec` to abort with SIGABRT.
  - **Valid**: `(allow file-read* (subpath "/usr") (subpath "/bin"))`
  - **Valid**: `(allow file-read* (subpath "/usr"))\n(allow file-read* (subpath "/bin"))`
  - **Invalid**: `(allow file-read*\n    (subpath "/usr")\n    (subpath "/bin"))`
- **R7.3.5**: Seatbelt profiles **must** use the following security model:
  - `(deny default)` - Deny everything by default
  - `(allow file-read*)` - Allow reading from all locations (shells need broad read access)
  - `(allow file-write* (subpath "<sandbox_scope>"))` - Restrict writes to sandbox scope only
  - `(allow file-write* (subpath "/private/tmp"))` - Allow temp file writes
  - `(allow file-write* (subpath "/private/var/folders"))` - Allow cache/temp writes
  - `(allow process*)` - Allow process operations
  - `(allow network*)` - Allow network access for tools like cargo
- **R7.3.6**: **macOS Path Symlinks**: `/var` is a symlink to `/private/var` on macOS. Seatbelt profiles **must** use real paths (e.g., `/private/var/folders` not `/var/folders`).

#### R7.4: Sandbox Prerequisite Validation

- **R7.4.1**: At server startup, the system **must** validate that the required sandboxing mechanism is available.
- **R7.4.2**: If the prerequisite is missing or unavailable, the server **must not** start. Instead, it **must**:
  1. Display a clear error message explaining the missing prerequisite
  2. Provide minimal instructions for how to install/enable it
  3. Explain why the server cannot start without security (risk of AI causing system damage outside the sandbox)
  4. Exit with a non-zero status code
- **R7.4.3**: The system **must not** automatically install prerequisites for the user.

#### R7.5: Deprecated Security Mechanisms

- **R7.5.1**: Command-line string pattern matching for path validation is **deprecated** and **must** be removed. Kernel-level enforcement is the only acceptable mechanism.
- **R7.5.2**: The previous `working_directory` parameter that allowed LLMs to specify arbitrary paths is **deprecated**. The sandbox scope is now set at initialization only.

#### R7.6: Nested Sandbox Environments

- **R7.6.1**: The system **must** detect when it is running inside another sandbox environment (e.g., Cursor, VS Code, Docker) where applying nested sandboxing (like `sandbox-exec` on macOS) would fail.
- **R7.6.2**: Upon detection, the system **must** exit with a clear error message instructing the user to disable the internal sandbox using the `--no-sandbox` flag or `AHMA_NO_SANDBOX=1` environment variable.
- **R7.6.3**: This "fail-secure" behavior ensures that users are aware that the internal sandbox is disabled and are relying on the outer environment's security.
- **R7.6.4**: The `README.md` claim of "graceful degradation" is superseded by this requirement for explicit user opt-in.

### R8: HTTP Bridge

The HTTP bridge exposes the MCP server over HTTP/SSE, enabling web clients and multi-session scenarios.

#### R8.1: Core HTTP Bridge

- **R8.1.1**: The system **must** support HTTP bridge mode via `ahma_mcp --mode http`.
- **R8.1.2**: The bridge **must** support Server-Sent Events (SSE) at `/mcp` (via HTTP GET) for server-to-client notifications.
- **R8.1.3**: The bridge **must** support JSON-RPC requests via POST at `/mcp`.
- **R8.1.4**: The bridge **must** handle concurrent requests by matching JSON-RPC IDs.
- **R8.1.5**: The bridge **must** auto-restart the stdio subprocess if it crashes.

#### R8.2: Content Negotiation

Per MCP protocol (rmcp 0.9.1+), clients select response format via `Accept` header:

- **R8.2.1**: `Accept: text/event-stream` → SSE streaming response.
- **R8.2.2**: `Accept: application/json` → Single JSON response.
- **R8.2.3**: No `Accept` header → Default to JSON for backward compatibility.
- **R8.2.4**: `Content-Type` response header **must** match actual format.
- **R8.2.5**: Per MCP Streamable HTTP Transport (2025-06-18), `/mcp` **must** accept both POST requests (for JSON-RPC) and GET requests (for SSE).

#### R8.3: Debug Output

In HTTP mode, terminal output echoes JSON for debugging:

- **R8.3.1**: JSON output **must** be pretty-printed with 2-space indentation.
- **R8.3.2**: Colored prefixes (`→ STDIN:`, `← STDOUT:`, `⚠ STDERR:`) **must** be preserved.
- **R8.3.3**: Non-JSON output printed as-is without modification.

#### R8.4: Session Isolation (Optional)

Per MCP specification (2025-03-26), `Mcp-Session-Id` header enables per-session sandbox isolation:

- **R8.4.1**: `--session-isolation` flag enables multi-session mode; without it, single-subprocess behavior is maintained.
- **R8.4.2**: In session isolation mode, each session gets a separate subprocess with its own sandbox scope.
- **R8.4.3**: Session ID (UUID) generated on `initialize` and returned via `Mcp-Session-Id` header.
- **R8.4.4**: Sandbox scope determined from first `roots/list` response (first root's URI).
- **R8.4.5**: Once set, sandbox scope **cannot** be changed (security invariant).
- **R8.4.6**: `notifications/roots/list_changed` after sandbox lock → session terminated, HTTP 403 Forbidden.
- **R8.4.7**: HTTP DELETE with `Mcp-Session-Id` terminates session and subprocess.
- **R8.4.8**: Session isolation is for local development only; no authentication provided.

See [docs/session-isolation.md](docs/session-isolation.md) for detailed architecture.

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
  "synchronous": true,
  "subcommand": [
    {
      "name": "subcommand_name",
      "description": "What this subcommand does. Include guidance for sync operations.",
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
      "description": "This subcommand runs async (default). Include async guidance here."
    }
  ]
}
```

### 3.2. Key Fields

- `command`: The base command-line executable (e.g., `git`, `cargo`).
- `subcommand`: An array of subcommands exposed as individual MCP tools. The final tool name will be `{command}_{name}` (e.g., `git_commit`).
- `enabled`: Controls whether the tool is active. Defaults to `true`. When set to `false`, the tool is **completely excluded** from the server: no availability probe is run, and it is not listed to MCP clients. Use this to disable tools that are not needed in the current environment without removing their definition.
- `synchronous`: Can be set at tool level or subcommand level. Subcommand-level overrides tool-level. Set to `true` for commands that must complete before dependent operations (e.g., `cargo add`). Omit or set to `false` for long-running commands that run asynchronously (the default).
- `options`: An array of command-line flags (e.g., `--release`).
- `positional_args`: An array of positional arguments.
- `format: "path"`: **CRITICAL**: Any option or argument that accepts a file path **must** include this for security validation.

### 3.3. Sequence Tools

Sequence tools chain multiple commands into a single workflow. In this repo, the preferred way to run multi-step workflows is to execute a shell pipeline via `sandboxed_shell`.

- Sequences are defined with `"command": "sequence"` and a `sequence` array.
- A configurable `step_delay_ms` prevents resource conflicts (e.g., `Cargo.lock` contention).

### 3.4. Tool Availability and Installation Hints

Tools can declare prerequisites and installation instructions:

- **`availability_check`**: Command to verify tool is installed (e.g., `which cargo-nextest`).
- **`install_instructions`**: Message shown when availability check fails.

## 4. Development Workflow

**For AI agent development instructions**, including:

- Setup commands
- Build and test commands
- Code style and conventions
- Testing requirements and patterns
- PR and commit guidelines
- Tool usage patterns

**See [AGENTS.md](AGENTS.md).**

This section previously contained development workflow details but has been moved to AGENTS.md to follow the agents.md standard. The REQUIREMENTS.md file now focuses purely on **what** the product does and **how** it's architected.

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

- **R10.4.1**: Tools that orchestrate multiple different tools (e.g., a `project_sequence` tool calling `sandboxed_shell` plus other tools) **must** define their sequence at the top level of the tool configuration.
- **R10.4.2**: Structure: `{"command": "sequence", "sequence": [{...}], "step_delay_ms": 500}`
- **R10.4.3**: Each sequence step specifies `tool` and `subcommand` to invoke.
- **R10.4.4**: Handled by `handle_sequence_tool()` in `mcp_service.rs`.
- **R10.4.5**: Sequence tools **must** be generic and reusable across projects. Project-specific validation or generation steps belong in dedicated project-specific sequence tools, not in generic tools.

#### Subcommand Sequences (Intra-Tool Workflows)

- **R10.4.6**: Subcommands that need to execute multiple steps within the same tool context **may** define a sequence at the subcommand level.
- **R10.4.7**: Structure: `{"subcommand": [{"name": "qualitycheck", "sequence": [{...}], "step_delay_ms": 500}]}`
- **R10.4.8**: Used for complex workflows within a single tool, invoked as `tool_subcommand` (e.g., `git_status`).
- **R10.4.9**: Handled by `handle_subcommand_sequence()` in `mcp_service.rs`.
- **R10.4.10**: **CRITICAL**: Subcommand names **must not** contain underscores, as underscores are used as hierarchical separators in the tool invocation system. For example, `git_status` maps to the `git` tool's `status` subcommand. Using `status_check` would cause parsing issues.

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

**R13.4**: Before committing, the full quality pipeline must pass. Prefer running via `sandboxed_shell`:

- `cargo fmt --all`
- `cargo clippy --all-targets`
- `cargo test` (or `cargo nextest run` if installed)

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

### R9: HTTP MCP Client and OAuth Authentication (Pending Integration)

- **R9.1**: `ahma_mcp` **must** be able to act as an MCP client for HTTP-based MCP servers, enabling it to connect to services like the Atlassian MCP server.
- **R9.2**: This functionality **is** implemented in the `ahma_http_mcp_client` crate within the Cargo workspace to maintain modularity as per principle R6.
- **R9.3**: The client **must** support the OAuth 2.0 authorization code flow with PKCE for user authentication.
- **R9.4**: When authentication is required, the system **must** provide the user with a URL to open in their web browser. The system attempts to open the browser automatically using the `webbrowser` crate; if that fails, it displays the link for the user to copy.
- **R9.5**: After successful user authentication in the browser, the client **must** handle the OAuth callback on `http://localhost:8080`, retrieve the authorization code, and exchange it for an access token and a refresh token using the `oauth2` crate. These tokens are stored in the system's temporary directory as `mcp_http_token.json`.
- **R9.6**: All subsequent requests to the MCP server **must** be authenticated using the stored access token via Bearer authentication. Token refresh logic is planned but not yet implemented.
- **R9.7**: The `mcp.json` configuration file **supports** definitions for HTTP-based MCP servers with the following structure:

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

- **R9.8**: The HTTP transport **implements** the `rmcp::transport::Transport<RoleClient>` trait, providing bidirectional communication:
  - **Sending**: HTTP POST requests with Bearer authentication for outgoing messages
  - **Receiving**: Server-Sent Events (SSE) for incoming messages from the server (background task)
- **R9.9**: **Current Status**: The `HttpMcpTransport` is fully implemented in the crate but **not yet integrated** into the main `ahma_mcp` binary. Integration is pending completion of rmcp 0.9.0 client API documentation and examples. The feature is currently disabled in `main.rs`.

### R15: Unified Shell Output (Added 2025-11-24)

- **R15.1**: All shell commands launched through `tokio::process::Command` **must** redirect stderr to stdout using `2>&1` so AI clients receive a single, chronologically ordered stream of output.
- **R15.2**: The adapter layer **must** automatically append `2>&1` to any shell script executed via `sh`, `bash`, or `zsh` `-c` invocations before the command is run.
- **R15.3**: Tests **must** cover this behavior to prevent regressions when new shell entry points are added.

### R16: Logging Configuration (Added 2025-11-26)

- **R16.1**: By default, the server **must** log to a daily rolling file in the user's cache directory to preserve log history without cluttering the terminal.
- **R16.2**: The `--log-to-stderr` flag **must** be supported to redirect all logging output to stderr instead of a file. This is useful for debugging and seeing errors in real-time during development.
- **R16.3**: When logging to stderr (via `--log-to-stderr`), ANSI color codes **must** be enabled on Mac and Linux to improve readability. Error messages should appear in red, warnings in yellow, etc.
- **R16.4**: When logging to a file, ANSI color codes **must** be disabled to keep log files clean and readable in text editors.
- **R16.5**: The `ahma-inspector.sh` script **must** use the `--log-to-stderr` flag by default so developers can see errors immediately in the terminal when testing with MCP Inspector.
- **R16.6**: The logging level **must** be controlled by the `--debug` flag (sets level to "debug") or default to "info" level.

### R17: MCP Callback Notifications (Added 2025-12-03)

Async operations push completion notifications to AI clients via MCP protocol callbacks:

- **R17.1**: When an async operation completes, the server **must** send an MCP progress notification containing the operation result.
- **R17.2**: The callback **must** include: operation ID, success/error status, and output content.
- **R17.3**: The callback system is implemented in `mcp_callback.rs` and integrates with `operation_monitor.rs`.
- **R17.4**: Callbacks use the MCP `notifications/progress` method per protocol specification.

### R18: List-Tools Mode (Added 2025-12-03)

For debugging and tool inspection, a list-tools mode is available:

- **R18.1**: `ahma_mcp --mode list-tools` **must** output all available tools as JSON to stdout.
- **R18.2**: The output **must** include tool name, description, and parameters for each tool.
- **R18.3**: The `ahma_list_tools` binary provides the same functionality as a standalone utility.

### R19: Protocol Stability & Cancellation Handling (Added 2025-12-13)

- **R19.1**: The system **must** distinguish between MCP protocol cancellations (client cancelling a request) and process cancellations.
- **R19.2**: When an MCP cancellation notification is received, the system **must** only cancel actual background operations (shell processes). It **must not** cancel synchronous MCP tool calls (like `await`, `status`, `cancel`) to prevent race conditions where the cancellation confirmation itself gets cancelled.
- **R19.3**: This logic is critical for stability with clients like Cursor that may send cancellation requests aggressively.

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

- **v0.7.0** (2025-12-03):
  - Consolidated R8 HTTP sections (R8, R8A-R8D → unified R8 with subsections)
  - Unified sequence tool handlers (`SequenceKind` enum, common helper functions)
  - Trimmed Section 4 Development Workflow
  - Added R3.4.1: Tool availability checks and install instructions
  - Added R17: MCP callback notifications for async operations
  - Added R18: List-tools mode for tool inspection
- **v0.6.3** (2025-11-26):
  - Added `--log-to-stderr` flag for colored terminal output (R16)
  - Updated `ahma-inspector.sh` to use `--log-to-stderr` by default
- **v0.6.2** (2025-11-26):
  - Added streaming response support for JSON/SSE content negotiation (R8.2)

### 6.3. Test Coverage Notes

Test coverage is tracked in the following areas:

- Cross-platform sandbox tests (macOS: Seatbelt, Linux: Landlock)
- Tool JSON configuration tests (`ahma_core/tests/tool_suite/`)
- SSE integration tests (`ahma_http_bridge/tests/sse_tool_integration_test.rs`)
- Session stress tests (`ahma_http_bridge/tests/session_stress_test.rs`)
- MCP service handler tests (`ahma_core/tests/mcp_service/`)

**Coverage Exemptions** (files excluded from 80% target):

- `ahma_core/src/test_utils.rs` — Testing infrastructure; testing test utilities creates circular dependencies
- `ahma_http_bridge/src/main.rs` — Binary entry point; tested via CLI integration tests

For detailed test documentation, see the test files directly.

---

**Last Updated**: 2025-12-13 (Updated R8 HTTP routes, R14 status, added R7.6 Nested Sandbox, R19 Cancellation)
**Status**: Living Document - Update with every architectural decision or significant change
