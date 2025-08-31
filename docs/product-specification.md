# Product Specification: Ahma MCP

**Version:** 1.0
**Date:** August 30, 2025
**Status:** In Development

## 1. Introduction

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for asynchronous and concurrent use by AI agents. Its core innovation is runtime introspection: it discovers a tool's complete interface at startup, eliminating the need for tool-specific, hardcoded logic.

This approach provides a consistent, powerful, and asynchronous bridge between AI and the virtually unlimited ecosystem of command-line utilities, enabling AI agents to work productively with any CLI tool without manual integration work.

### 1.1. Key Capabilities

- **Configuration-Driven Tool Definition**: `ahma_mcp` uses TOML files as the source of truth to define a tool's interface, including its subcommands and options. This provides a stable and explicit way to generate MCP tool schemas.
- **Dedicated Subcommand Tools**: Exposes each subcommand as a distinct MCP tool (e.g., `cargo_build`, `cargo_test`), providing a clear and unambiguous interface for the AI client.
- **Asynchronous-First Architecture**: All operations default to asynchronous execution, enabling AI to initiate multiple long-running tasks and continue productive work without blocking.
- **Intelligent AI Guidance**: Provides context-aware "tool hints" that guide AI agents toward concurrent thinking patterns instead of passive waiting.
- **High-Performance Execution**: Uses a pre-warmed shell pool architecture for minimal command startup latency and optimal concurrent operation handling.

## 2. Architecture

`ahma_mcp` is built on a modular architecture that combines a generic command execution engine with an explicit, configuration-driven MCP interface.

### 2.1. Core Components

1.  **Configuration Loader**: Reads `tools/*.toml` files at startup. These files are the definitive source for tool definitions.
2.  **MCP Schema Generator**: Translates the subcommand and option definitions from the TOML files into distinct MCP tool schemas (e.g., one for `cargo_build`, one for `cargo_test`).
3.  **MCP Server Interface**: Handles JSON-RPC 2.0 communication, routing incoming requests like `cargo_build` to the appropriate handler.
4.  **Asynchronous Command Executor**: Manages a high-performance shell pool to execute commands. It tracks operations via unique IDs.
5.  **Operation Monitor & Callback System**: Tracks the lifecycle of all asynchronous operations and automatically pushes results back to the AI client upon completion.

### 2.2. Data Flow: Startup and Tool Adaptation

1.  `ahma_mcp` is launched, scanning the `tools/` directory.
2.  The **Configuration Loader** reads `tools/cargo.toml`.
3.  The TOML file defines the subcommands (`build`, `check`, `test`) and their respective options.
4.  The **MCP Schema Generator** iterates through these definitions. For each subcommand, it generates a complete MCP tool schema (e.g., a `cargo_build` tool with properties for `--release`, `--jobs`, etc.).
5.  The **MCP Server** registers all these generated tools, making them available to the client.

### 2.3. Data Flow: Asynchronous Operation

1.  The AI sends a tool request, e.g., `{"tool_name": "cargo_build", "arguments": {"release": true, "working_directory": "/path/to/project", "enable_async_notification": true}}`.
2.  The server validates the request against the `cargo_build` schema, creates a unique `operation_id`, and immediately returns it to the AI.
3.  Simultaneously, it generates and sends a "tool hint," e.g., "While `cargo build --release` is running, consider planning the next steps for deployment or writing documentation."
4.  The **Asynchronous Command Executor** runs `cargo build --release` in a background shell.
5.  The AI, unblocked, proceeds with other tasks.
6.  When the command completes, the **Callback System** pushes the result (stdout, stderr, exit code) to the AI, linked by the `operation_id`.

## 3. Command-Line Interface

```bash
ahma_mcp --tools-dir <PATH_TO_TOOLS_DIR> [OPTIONS]

Required:
  --tools-dir <PATH>     Path to the directory containing tool configuration TOML files.

Options:
  --synchronous          Force all operations to run synchronously.
  --help                 Print help information.
```

## 4. Configuration File (`tools/*.toml`)

The TOML file is the central point of configuration, defining the tool's structure.

### 4.1. Example Configuration

This example defines the `cargo` tool and two of its subcommands.

```toml
# tools/cargo.toml

# The base command to execute.
command = "cargo"

# Define each subcommand that should be exposed as an MCP tool.
[[subcommand]]
name = "build"
description = "Compile the current package. Tip: use enable_async_notification=true."

# Define the options for the 'build' subcommand.
# The 'type' can be 'boolean', 'string', 'integer', or 'array'.
options = [
    { name = "release", type = "boolean", description = "Build artifacts in release mode, with optimizations" },
    { name = "jobs", type = "integer", description = "Number of parallel jobs, defaults to # of CPUs" },
    { name = "features", type = "array", description = "Space or comma separated list of features to activate" }
]

[[subcommand]]
name = "test"
description = "Run the tests. Tip: use enable_async_notification=true."

options = [
    { name = "release", type = "boolean", description = "Build artifacts in release mode, with optimizations" },
    { name = "no-fail-fast", type = "boolean", description = "Run all tests regardless of failure" }
]

# [Custom Tool Hints]
# You can override the default, automatically generated hints here.
[hints]
build = "While the project is building, review the test plan or update the documentation."
test = "While the tests are running, you can start drafting the summary of the changes."
```

## 5. Future Enhancements

- **Web Tool Adaptation**: Extend the dynamic adaptation concept to web APIs by parsing OpenAPI/Swagger specifications.
- **Interactive Tool Support**: Develop a mechanism for handling interactive commands that prompt for user input.
- **Visual Operation Tracker**: A VS Code extension to provide a UI for monitoring active background operations.

## 6. Developer Setup Notes

- Build a release binary for use with VS Code MCP: `cargo build --release`.
- Use absolute paths for `cwd` in your `mcp.json`.
- The shell pool is configurable via `ShellPoolConfig`; defaults aim for good latency with low resource use.

## 7. Cargo Command Parity

The server aims to expose (subject to `cargo --help` output on the host):

- Core: build, run, test, check, clean, doc, add, remove, update, fetch, install, search, tree, version, rustc, metadata.
- Optional (if installed): clippy, nextest, fmt, audit, upgrade, bump_version, bench.
