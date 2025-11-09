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

### R2: Async-First Architecture

- **R2.1**: Operations **must** execute asynchronously by default. When an AI invokes a tool, the server immediately returns an `operation_id` and a `started` status, then executes the command in the background.
- **R2.2**: Upon completion of an asynchronous operation, the system **must** automatically push the final result (success or failure) to the AI client via an MCP progress notification.
- **R2.3**: The AI client **should not** poll for results. The architecture is designed to push results automatically.
- **R2.4**: Tool descriptions for async operations **must** explicitly guide the AI to continue with other tasks and not to wait, processing the result notification when it arrives.

### R3: Selective Synchronous Override

- **R3.1**: Fast, non-blocking operations (e.g., `git status`, `cargo version`) **can** be marked as synchronous in their JSON configuration (`"synchronous": true`).
- **R3.2**: Synchronous operations **must** block, execute, and return their final result directly in a single response. They do not use operation IDs or send completion notifications.
- **R3.3**: Launching `ahma_mcp` with the `--synchronous` flag **must** override all tool configuration defaults for that session, forcing every tool invocation to execute synchronously.

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
- **R6.5**: The MTDF JSON Schema generation logic **must** be implemented in the `generate_schema` binary crate.
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
      "description": "What this subcommand does. Must include async guidance if not synchronous.",
      "synchronous": false,
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
- `synchronous`: `false` for async (default), `true` for sync.
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

1. **`generate_schema`**: Regenerates the MTDF JSON schema to ensure it is up-to-date with any changes in the core data structures.
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

### 4.3. Copilot CLI Verification

- **R9.1**: Copilot LLMs **should** validate code and tool changes by invoking `ahma_mcp` directly from the command line using the `--tool_name` and `--tool_args` parameters. This keeps validation steps reproducible and scriptable during autonomous runs.
- **R9.2**: Always pass the MCP tool identifier to `--tool_name`, and supply the exact arguments that would normally be provided through the MCP interface via `--tool_args`.
- **R9.3**: Use the double-dash (`--`) separator within `--tool_args` to forward raw positional arguments exactly as the target CLI expects when necessary.
- **Example – rebuild after code changes**: `ahma_mcp --tool_name cargo_build --tool_args -- --release`
- **Example – run quality checks**: `ahma_mcp rust_quality_check`
