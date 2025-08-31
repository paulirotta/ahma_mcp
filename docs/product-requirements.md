# Product Requirements for Ahma MCP

This document outlines the high-level requirements for `ahma_mcp`, a universal, config-driven adapter for exposing command-line tools to AI agents via the Model Context Protocol (MCP).

## 1. Core Functionality

- **R1.1**: The system shall adapt any command-line tool for use as a set of MCP tools based on a declarative TOML configuration file.
- **R1.2**: The primary mode of operation shall be asynchronous, allowing an AI agent to execute long-running commands without blocking. The system will immediately return an operation ID and optional tool hints.
- **R1.3**: The system shall support a synchronous execution mode, either globally via a command-line flag (`--synchronous`) or on a per-subcommand basis via the TOML configuration. In this mode, the system blocks until the command completes and returns the result directly.

## 2. Configuration and Tool Definition

- **R2.1**: All tools shall be defined in `.toml` files located in a specified `tools/` directory.
- **R2.2**: The system shall scan this directory at startup and load all valid and enabled tool configurations.
- **R2.3**: The `Config` structure (`src/config.rs`) shall define the schema for these files, including the base command, a list of subcommands, and their options.
- **R2.4**: Each subcommand defined in the TOML will be exposed as a distinct MCP tool (e.g., `cargo build` becomes the `cargo_build` tool).

## 3. Dynamic Schema Generation

- **R3.1**: For each subcommand in a tool's configuration, the system shall generate a corresponding MCP tool definition.
- **R3.2**: The tool name shall be a combination of the base command and the subcommand name (e.g., `git_commit`).
- **R3.3**: The `input_schema` for each tool shall be dynamically generated based on the `options` defined in the `Subcommand` struct in the TOML file. This provides a strongly-typed interface for the AI client.
- **R3.4**: The system must support basic data types for options, such as `boolean`, `string`, `integer`, and `array`.

## 4. Asynchronous Operation and AI Interaction

- **R4.1**: In asynchronous mode, invoking a tool shall immediately return a unique operation ID to the AI client.
- **R4.2**: The system should provide a mechanism for pushing operation results back to the AI client upon completion (e.g., via MCP progress notifications, though this is currently handled by the client polling with the operation ID).
- **R4.3**: The system shall provide "tool hints" to the AI agent after initiating an asynchronous operation. These hints, defined in the TOML configuration, suggest productive next steps for the agent to take while waiting.
- **R4.4**: Tool hints are only sent for asynchronous operations.

## 5. Synchronous Operation

- **R5.1**: A global `--synchronous` flag shall force all commands to execute in a blocking manner, returning the final result directly.
- **R5.2**: A `synchronous = true` flag can be set on any `Subcommand` in the TOML configuration to force that specific command to run synchronously, overriding the global default.
- **R5.3**: When a command is run synchronously, no operation ID or tool hints are returned; only the final `stdout` or `stderr` is provided upon completion.

## 6. Performance and Reliability

- **R6.1**: The system shall use a high-performance shell pool (`ShellPoolManager`) to minimize command startup latency for asynchronous commands executed within a known working directory.
- **R6.2**: Operations shall be isolated to prevent interference between concurrent commands.
- **R6.3**: The system shall provide robust error handling, clearly reporting command failures, timeouts, and configuration errors.
- **R6.4**: Resource usage shall be optimized to handle multiple concurrent operations efficiently.

## 7. Extensibility and Maintainability

- **R7.1**: The architecture shall be modular, with a clear separation of concerns between configuration (`config.rs`), MCP service logic (`mcp_service.rs`), and command execution (`adapter.rs`).
- **R7.2**: Adding new tools or modifying existing ones should be achievable purely by editing TOML files, with no changes to the core Rust codebase.
