# Product Specification: Ahma MCP

**Version:** 1.0
**Date:** August 30, 2025
**Status:** In Development

## 1. Introduction

`ahma_mcp` is a universal, high-performance Model Context Protocol (MCP) server designed to dynamically adapt any command-line tool for asynchronous and concurrent use by AI agents. Its core innovation is runtime introspection: it discovers a tool's complete interface at startup, eliminating the need for tool-specific, hardcoded logic.

This approach provides a consistent, powerful, and asynchronous bridge between AI and the virtually unlimited ecosystem of command-line utilities, enabling AI agents to work productively with any CLI tool without manual integration work.

### 1.1. Key Capabilities

- **Dynamic Tool Introspection**: At startup, `ahma_mcp` automatically parses the `--help` output of target tools to generate comprehensive and accurate MCP tool schemas.
- **Unified Tool Interface**: Exposes a single, intelligent MCP tool per adapted CLI application (e.g., one `cargo` tool instead of separate `cargo-build`, `cargo-test` tools), with subcommands and options as structured parameters.
- **Asynchronous-First Architecture**: All operations default to asynchronous execution, enabling AI to initiate multiple long-running tasks and continue productive work without blocking.
- **Intelligent AI Guidance**: Provides automatically generated, context-aware "tool hints" that guide AI agents toward concurrent thinking patterns instead of passive waiting.
- **Self-Maintaining Configuration**: Automatically maintains user TOML configuration files with live examples of discovered commands and options, providing always-current customization templates.
- **High-Performance Execution**: Uses a pre-warmed shell pool architecture for minimal command startup latency and optimal concurrent operation handling.
  - Implementation note: A configurable ShellPoolManager maintains per-directory pools with health checks and idle cleanup (see `src/shell_pool.rs`).

## 2. Architecture

`ahma_mcp` is built on a modular architecture that borrows proven concepts from `async_cargo_mcp` but replaces static command definitions with a dynamic, introspective core.

### 2.1. Core Components

1.  **Configuration Loader**: Reads a `tools/*.toml` file specified at startup. This file dictates which command-line tool to adapt.
2.  **CLI Introspector**: The "brain" of `ahma_mcp`. At startup, it executes the target command with `--help` (and recursively for subcommands) and parses the output to build an in-memory model of the tool's structure.
3.  **MCP Schema Generator**: Translates the in-memory model of the CLI tool into a single, rich JSON schema for the unified MCP tool.
4.  **MCP Server Interface**: Handles JSON-RPC 2.0 communication with the AI client.
5.  **Asynchronous Command Executor**: Manages a high-performance shell pool (adapted from `async_cargo_mcp`) to execute commands. It tracks operations via unique IDs.
6.  **Operation Monitor & Callback System**: Tracks the lifecycle of all asynchronous operations and automatically pushes results back to the AI client upon completion.
7.  **Configuration Manager**: Responsible for safely updating the user's TOML configuration file with discovered tool information.

### 2.2. Data Flow: Startup and Tool Adaptation

1.  `ahma_mcp` is launched with a tool configuration file, for example `--config tools/cargo.toml`.
2.  The **Configuration Loader** reads `tools/cargo.toml` and identifies `command = "cargo"`.
3.  The **CLI Introspector** runs `cargo --help` and parses the list of subcommands (e.g., `build`, `check`, `test`).
4.  For each subcommand, it runs `cargo <subcommand> --help` to discover its specific options and flags.
5.  The **MCP Schema Generator** uses this data to construct a JSON schema for a single tool named `cargo`. This schema includes a parameter (e.g., `subcommand: "build" | "check" | "test"`) and nested objects for the options of each subcommand.
6.  The **Configuration Manager** reads `tools/cargo.toml`, compares its contents to the discovered structure, and appends any new or changed subcommands/options as commented-out TOML sections.

### 2.3. Data Flow: Asynchronous Operation

1.  The AI sends a tool request, e.g., `{"tool_name": "cargo", "subcommand": "build", "release": true, "enable_async_notification": true}`.
2.  The server validates the request against the generated schema, creates a unique `operation_id`, and immediately returns it to the AI.
3.  Simultaneously, it generates and sends a "tool hint," e.g., "While `cargo build --release` is running, consider planning the next steps for deployment or writing documentation."
4.  The **Asynchronous Command Executor** runs `cargo build --release` in a background shell.
5.  The AI, unblocked, proceeds with other tasks.
6.  When the command completes, the **Callback System** pushes the result (stdout, stderr, exit code) to the AI, linked by the `operation_id`.

## 3. Command-Line Interface

```bash
ahma_mcp --config <PATH_TO_TOML> [OPTIONS]

Required:
  --config <PATH>        Path to the tool configuration TOML file.

Options:
  --synchronous          Force all operations to run synchronously.
  --help                 Print help information.
```

## 4. Configuration File (`tools/*.toml`)

The TOML file is the central point of configuration.

### 4.1. Minimal Configuration

This is all that's needed to get started.

```toml
# tools/my_cli.toml
command = "my_cli"
```

### 4.2. Customization and Auto-Generated Documentation

`ahma_mcp` will enrich this file over time. The user can uncomment and edit sections to customize behavior.

```toml
# tools/cargo.toml

# [Core Configuration]
# This is the only required field.
command = "cargo"

# [Custom Tool Hints]
# The system automatically generates hints. You can override them here.
# If a hint is not specified for a command, a default one will be used.
[hints]
build = "While the project is building, review the test plan or update the documentation."
test = "While the tests are running, you can start drafting the summary of the changes."
# clippy = "..." # No custom hint, so default will be used.

#=======================================================================#
# [Discovered Tool Structure]                                           #
# The section below is automatically generated and updated by ahma_mcp. #
# Do not edit it directly. Uncomment and move sections to               #
# '[Custom Tool Hints]' or other configuration sections to override.    #
#=======================================================================#

# [discovered.build]
# description = "Compile the current package"
# options = ["--release", "--jobs <N>", ...]

# [discovered.test]
# description = "Run the tests"
# options = ["--no-fail-fast", "-- --nocapture", ...]
```

### 4.3. Conflict Resolution

The system will use the following logic to avoid overwriting user changes:

- It will only ever write to the commented-out `[discovered.*]` sections.
- If a user has a custom `[hints.build]` section, the system will not attempt to modify it. It will only manage the corresponding `# [discovered.build]` section.
- This design separates user-managed configuration from system-managed documentation, preventing conflicts.

## 5. Future Enhancements

- **Web Tool Adaptation**: Extend the dynamic adaptation concept to web APIs by parsing OpenAPI/Swagger specifications.
- **Interactive Tool Support**: Develop a mechanism for handling interactive commands that prompt for user input.
- **Visual Operation Tracker**: A VS Code extension to provide a UI for monitoring active background operations.

## 6. Developer Setup Notes

- Build a release binary for use with VS Code MCP: `cargo build --release`.
- Use absolute paths for `cwd`, `command`, and `args` in your `mcp.json`.
- The shell pool is configurable via `ShellPoolConfig`; defaults aim for good latency with low resource use.

## 7. Cargo Command Parity

The server aims to expose (subject to `cargo --help` output on the host):

- Core: build, run, test, check, clean, doc, add, remove, update, fetch, install, search, tree, version, rustc, metadata.
- Optional (if installed): clippy, nextest, fmt, audit, upgrade, bump_version, bench.
