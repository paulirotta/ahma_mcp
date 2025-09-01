# Product Specification: Ahma MCP

**Version:** 1.1
**Date:** August 31, 2025
**Status:** In Development

## 1. Introduction

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for use by AI agents. Its core principle is **async-first execution with automatic result push**: it uses simple JSON files to define tool interfaces and automatically pushes results back to AI clients when operations complete, eliminating blocking and maximizing AI productivity.

This approach provides a consistent, powerful, and non-blocking bridge between AI and the vast ecosystem of command-line utilities, enabling AI agents to work concurrently on multiple tasks while operations execute in the background.

### 1.1. Key Capabilities

- **Async-First Architecture**: Operations execute asynchronously by default with automatic MCP progress notifications when complete, eliminating AI blocking and enabling concurrent workflows.
- **Configuration-Driven Tool Definition**: Uses `.json` files as the single source of truth to define a tool's interface, including sync/async behavior, timeouts, and AI guidance.
- **High-Performance Shell Pool**: Pre-warmed shell processes provide 10x faster command startup (5-20ms vs 50-200ms), optimizing both synchronous and asynchronous operations.
- **AI Guidance Integration**: Tool descriptions include explicit instructions to AI clients about optimal behavior, promoting productive parallel work over waiting.
- **Selective Synchronous Override**: Fast operations (status, version) can be marked synchronous in JSON for immediate results without notifications.

## 2. Architecture

`ahma_mcp` is built on a modular architecture that combines a generic command execution engine with an explicit, configuration-driven MCP interface.

### 2.1. Core Components

1.  **Configuration Loader (`main.rs`, `config.rs`)**: Reads all `tools/*.json` files at startup. These files are the definitive source for all tool definitions.
2.  **MCP Service & Schema Generator (`mcp_service.rs`)**:
    - Iterates through the loaded configurations.
    - For each `[[subcommand]]` entry in a JSON file, it dynamically generates a complete MCP tool definition (e.g., a `cargo_build` tool with an `input_schema` derived from its defined `options`).
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

### 2.3. Data Flow: Asynchronous Operation (Default)

1.  The AI sends a tool request, e.g., `{"tool_name": "cargo_build", "arguments": {"release": true, "working_directory": "/path/to/project"}}`.
2.  The server validates the request against the `cargo_build` schema.
3.  The **Command Executor (`Adapter`)** immediately returns a response containing an `operation_id` and `status: "started"`.
4.  A background task spawns to execute the command (`cargo build --release`) using the **Shell Pool Manager**.
5.  The AI receives the immediate response and can proceed with other productive tasks.
6.  When the background command completes, the system automatically pushes an MCP progress notification with the final result to the AI client.
7.  The AI processes the completion notification and takes appropriate action based on success or failure.

### 2.4. Data Flow: Synchronous Operation (Override)

1.  The AI sends a tool request for a command marked as `synchronous = true` in its JSON config, e.g., `git_status`.
2.  The server validates the request.
3.  The **Command Executor (`Adapter`)** identifies the command as synchronous and executes it directly using the shell pool for performance.
4.  Once the command completes, the server returns the final result (`stdout` or `stderr`) directly to the client. No `operation_id` or notifications are sent.

## 3. Command-Line Interface

```bash
ahma_mcp [OPTIONS]

Options:
  --tools-dir <PATH>     Path to the directory containing tool JSON files.
                         (Default: 'tools')
  --timeout <SECONDS>    Default timeout for commands in seconds. (Default: 300)
  --debug                Enable debug-level logging.
  --server               Run in persistent MCP server mode (required).
  --help                 Print help information.
```

**Note**: The `--synchronous` flag has been removed. Synchronous vs asynchronous behavior is now determined exclusively by JSON configuration to eliminate AI confusion and optimize productivity.

## 4. Configuration File (`tools/*.json`)

The JSON file is the central point of configuration, defining a tool's structure.

### 4.1. Example Configuration (`tools/cargo.json`)

```json
{
  "command": "cargo",
  "enabled": true,
  "timeout_seconds": 600,
  "hints": {
    "default": "While cargo operations run, consider reviewing Cargo.toml dependencies, planning tests, or analyzing code structure for optimization opportunities.",
    "build": "Building in progress - review compilation output for warnings, plan deployment steps, or work on documentation.",
    "test": "Tests running - analyze test patterns, consider additional test cases, or review code coverage strategies."
  },
  "subcommand": [
    {
      "name": "build",
      "description": "Compile the current package.\n\n**IMPORTANT:** This tool operates asynchronously.\n1. **Immediate Response:** Returns operation_id and status 'started'. NOT success.\n2. **Final Result:** Result pushed automatically via MCP notification when complete.\n\n**Your Instructions:**\n- **DO NOT** wait for the final result.\n- **DO** continue with other tasks that don't depend on this operation.\n- You **MUST** process the future result notification to know if operation succeeded.",
      "options": [
        {
          "name": "release",
          "type": "boolean",
          "description": "Build artifacts in release mode, with optimizations"
        },
        {
          "name": "jobs",
          "type": "integer",
          "description": "Number of parallel jobs, defaults to # of CPUs"
        },
        {
          "name": "features",
          "type": "array",
          "description": "Space or comma separated list of features to activate"
        }
      ]
    },
    {
      "name": "version",
      "description": "Show cargo version information - returns immediately.",
      "synchronous": true,
      "options": []
    },
    {
      "name": "wait",
      "description": "Wait for previously started asynchronous operations to complete.\n\n**WARNING:** This is a blocking tool and makes you inefficient.\n- **ONLY** use this if you have NO other tasks and cannot proceed until completion.\n- It is **ALWAYS** better to perform other work and let results be pushed to you.\n- Use this ONLY for final project validation when all work is complete.",
      "synchronous": true,
      "options": [
        {
          "name": "operation_ids",
          "type": "array",
          "description": "Specific operation IDs to wait for"
        }
      ]
    }
  ]
}
```

## 5. Future Enhancements

- **Interactive Tool Support**: Develop a mechanism for handling interactive commands that prompt for user input.
- **Server-Side Operation Tracking**: Re-introduce a server-side `OperationMonitor` to track the status of async jobs, allowing for `wait` and `status` tools to query job progress from the server itself, rather than relying on the client.
- **Automatic Callback on Completion**: Implement a true callback system where the server proactively pushes results to the client upon async completion, removing the need for client-side polling.
