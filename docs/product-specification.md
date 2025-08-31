# Product Specification: Ahma MCP

**Version:** 1.1
**Date:** August 31, 2025
**Status:** In Development

## 1. Introduction

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for use by AI agents. Its core principle is **configuration over code**: it uses simple TOML files to define a tool's complete interface, eliminating the need for tool-specific, hardcoded logic.

This approach provides a consistent, powerful, and maintainable bridge between AI and the vast ecosystem of command-line utilities, enabling AI agents to work productively with any CLI tool through declarative definitions.

### 1.1. Key Capabilities

- **Configuration-Driven Tool Definition**: `ahma_mcp` uses `.toml` files as the single source of truth to define a tool's interface, including its base command, subcommands, and their options.
- **Dedicated Subcommand Tools**: It exposes each subcommand as a distinct MCP tool (e.g., `cargo build` becomes `cargo_test`), providing a clear and unambiguous interface for the AI client.
- **Flexible Sync/Async Architecture**: Operations can be run asynchronously (the default) or synchronously. This can be controlled globally via a CLI flag or on a per-subcommand basis within the TOML configuration.
- **Intelligent AI Guidance**: For asynchronous operations, it provides context-aware "tool hints" that guide AI agents toward productive, concurrent workflows instead of passive waiting.
- **High-Performance Execution**: It uses a pre-warmed shell pool (`ShellPoolManager`) for minimal command startup latency, optimizing concurrent operation handling.

## 2. Architecture

`ahma_mcp` is built on a modular architecture that combines a generic command execution engine with an explicit, configuration-driven MCP interface.

### 2.1. Core Components

1.  **Configuration Loader (`main.rs`, `config.rs`)**: Reads all `tools/*.toml` files at startup. These files are the definitive source for all tool definitions.
2.  **MCP Service & Schema Generator (`mcp_service.rs`)**:
    - Iterates through the loaded configurations.
    - For each `[[subcommand]]` entry in a TOML file, it dynamically generates a complete MCP tool definition (e.g., a `cargo_build` tool with an `input_schema` derived from its defined `options`).
    - It registers all generated tools with the MCP server, making them available to the client.
3.  **Request Router (`mcp_service.rs`)**: Handles incoming JSON-RPC 2.0 requests, validating them against the generated schemas and dispatching them to the command executor.
4.  **Command Executor (`adapter.rs`)**:
    - The generic engine for running commands. It is entirely tool-agnostic.
    - It determines whether to run a command synchronously or asynchronously based on the global `--synchronous` flag and the specific subcommand's configuration.
    - For async operations, it leverages the `ShellPoolManager`.
5.  **Shell Pool Manager (`shell_pool.rs`)**: Manages a pool of pre-warmed, reusable shell processes to execute commands with low latency. This is primarily used for asynchronous commands that specify a working directory.

### 2.2. Data Flow: Startup and Tool Adaptation

1.  `ahma_mcp` is launched, scanning the `tools/` directory.
2.  The **Configuration Loader** reads `tools/cargo.toml`, `tools/git.toml`, etc.
3.  For each file, it parses the `command` and the list of `[[subcommand]]` definitions.
4.  The **MCP Service** iterates through these definitions. For each subcommand, it generates a complete MCP tool schema (e.g., a `cargo_build` tool with properties for `--release`, `--jobs`, etc.).
5.  The MCP server registers all these generated tools, making them available to the client.

### 2.3. Data Flow: Asynchronous Operation

1.  The AI sends a tool request, e.g., `{"tool_name": "cargo_build", "arguments": {"release": true, "working_directory": "/path/to/project"}}`.
2.  The server validates the request against the `cargo_build` schema.
3.  Because the command is asynchronous by default, the server immediately returns a response containing an `operation_id` and any configured `hints`.
4.  The **Command Executor (`Adapter`)** dispatches the command (`cargo build --release`) to the **Shell Pool Manager**, which runs it in a background shell.
5.  The AI, unblocked, can proceed with other tasks, using the hints for guidance.
6.  The client is responsible for polling for the result using the `operation_id`.

### 2.4. Data Flow: Synchronous Operation

1.  The AI sends a tool request for a command marked as `synchronous = true` in its TOML config, e.g., `git_status`.
2.  The server validates the request.
3.  The **Command Executor (`Adapter`)** identifies the command as synchronous. It blocks and executes the command directly.
4.  Once the command completes, the server returns the final result (`stdout` or `stderr`) directly to the client. No `operation_id` or `hints` are sent.

## 3. Command-Line Interface

```bash
ahma_mcp [OPTIONS]

Options:
  --tools-dir <PATH>     Path to the directory containing tool TOML files.
                         (Default: 'tools')
  --synchronous          Force ALL operations to run synchronously, overriding
                         individual tool configurations.
  --timeout <SECONDS>    Default timeout for commands in seconds. (Default: 300)
  --debug                Enable debug-level logging.
  --help                 Print help information.
```

## 4. Configuration File (`tools/*.toml`)

The TOML file is the central point of configuration, defining a tool's structure.

### 4.1. Example Configuration (`tools/cargo.toml`)

```toml
# The base command to execute.
command = "cargo"
# Optional: Is this tool enabled? Defaults to true.
enabled = true

# Optional: Global hints for this tool.
[hints]
default = "While waiting for the cargo command, you could review the project's dependencies in Cargo.toml."

# Define each subcommand that should be exposed as an MCP tool.
[[subcommand]]
name = "build"
description = "Compile the current package."
# This command will run asynchronously by default.

# Define the options for the 'build' subcommand.
# The 'type' can be 'boolean', 'string', 'integer', or 'array'.
options = [
    { name = "release", type = "boolean", description = "Build artifacts in release mode, with optimizations" },
    { name = "jobs", type = "integer", description = "Number of parallel jobs, defaults to # of CPUs" },
    { name = "features", type = "array", description = "Space or comma separated list of features to activate" }
]

[[subcommand]]
name = "status"
description = "Show the working tree status (equivalent to 'git status')."
# This command should be fast, so we'll make it synchronous.
synchronous = true

options = [
    { name = "short", type = "boolean", description = "Give the output in the short-format" }
]
```

## 5. Future Enhancements

- **Interactive Tool Support**: Develop a mechanism for handling interactive commands that prompt for user input.
- **Server-Side Operation Tracking**: Re-introduce a server-side `OperationMonitor` to track the status of async jobs, allowing for `wait` and `status` tools to query job progress from the server itself, rather than relying on the client.
- **Automatic Callback on Completion**: Implement a true callback system where the server proactively pushes results to the client upon async completion, removing the need for client-side polling.
