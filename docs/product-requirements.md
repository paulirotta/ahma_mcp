# Product Requiremen- **R3.3**: Based on the discovered information, the system shall dynamically generate a dedicated MCP tool for each discovered subcommand (e.g., `cargo_build`, `cargo_test`). This provides a clear and direct interface for the AI client.s for Ahma MCP

This document outlines the high-level requirements for `ahma_mcp`, a universal adapter for command-line tools to be used with AI via the Model Context Protocol (MCP).

## 1. Core Functionality

- **R1.1**: The system shall dynamically adapt any given command-line interface (CLI) tool for use as an MCP tool without requiring tool-specific knowledge or configuration.
- **R1.2**: The primary mode of operation shall be asynchronous, allowing the AI to execute long-running commands in the background and continue with other productive tasks.
- **R1.3**: The system shall provide an optional synchronous execution mode for users who prefer a simpler, blocking workflow or for commands that require immediate results.

## 2. Configuration and Discoverability

- **R2.1**: The CLI tool to be adapted must be specified via a TOML configuration file with minimal required configuration.
- **R2.2**: The path to this configuration file must be provided as a required command-line argument to `ahma_mcp` at startup.
- **R2.3**: The minimal configuration required shall be only the command name (e.g., `command = "cargo"`), with all other capabilities discovered dynamically.
- **R2.4**: The system shall validate that the specified CLI tool is available and executable on the host system before proceeding.

## 3. Dynamic Tool Discovery and Schema Generation

- **R3.1**: At startup, the system shall inspect the target CLI tool to discover its available subcommands, options, and flags by parsing `--help` output and related documentation.
- **R3.2**: Based on the discovered information, the system shall dynamically generate a single, unified JSON schema for the MCP tool that accurately represents all available functionality.
- **R3.3**: This single MCP tool shall represent the entire command-line application, with subcommands exposed as enumerated parameters within the unified tool schema.
- **R3.4**: The generated schema must be version-aware, ensuring compatibility with the specific version of the tool installed on the host system.
- **R3.5**: The system shall gracefully handle parsing errors and provide fallback mechanisms when help text cannot be fully parsed.

## 4. Asynchronous Operation and AI Interaction

- **R4.1**: In asynchronous mode, tool invocation shall immediately return a unique operation ID to the AI client.
- **R4.2**: The system shall automatically push operation results back to the AI client upon completion using MCP progress notifications.
- **R4.3**: To encourage concurrent thinking, the system shall provide intelligent "tool hints" to the AI after initiating asynchronous operations, suggesting productive tasks to perform while waiting.
- **R4.4**: Tool hints shall be context-aware, taking into account the specific command being executed and the current project state.

## 5. Configuration Management and Customization

- **R5.1**: The system shall support extensive customization through the tool's TOML configuration file while maintaining backward compatibility.
- **R5.2**: Users shall be able to override default tool hints, command timeouts, and other behavior on a per-command basis.
- **R5.3**: The system shall automatically update the TOML configuration file with discovered tool information as commented examples, providing users with current templates for customization.
- **R5.4**: Configuration updates shall be non-destructive, preserving user customizations while adding new discovered capabilities.
- **R5.5**: The system must provide clear conflict resolution when user modifications conflict with automatic updates, including detailed error messages and suggested resolutions.

## 6. Performance and Reliability

- **R6.1**: The system shall use a high-performance shell pool architecture to minimize command startup latency.
- **R6.5**: Provide clear documentation and examples for configuring VS Code MCP with absolute paths and using a pre-built release binary.
- **R6.2**: Operations shall be isolated to prevent interference between concurrent commands.
- **R6.3**: The system shall provide comprehensive error handling and recovery mechanisms.
- **R6.4**: Resource usage shall be optimized to handle multiple concurrent operations efficiently.

## 7. Extensibility and Future Growth

- **R7.1**: The architecture shall be modular and extensible to support future adaptation of web APIs and other tool types.
- **R7.2**: The system shall provide clear interfaces for adding new discovery mechanisms and output parsers.
- **R7.3**: Tool hint generation shall be pluggable to allow for domain-specific customization.

## 8. Cargo Command Parity (with async_cargo_mcp)

- Core: build, run, test, check, clean, doc, add, remove, update, fetch, install, search, tree, version, rustc, metadata.
- Optional (if installed): clippy, nextest, fmt, audit, upgrade, bump_version, bench.
