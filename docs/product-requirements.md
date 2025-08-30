# Product Requirements for Ahma MCP

This document outlines the high-level requirements for `ahma_mcp`, a universal adapter for command-line tools to be used with AI via the Model Context Protocol (MCP).

## 1. Core Functionality

- **R1.1**: The system shall adapt any given command-line interface (CLI) tool for use as an MCP tool.
- **R1.2**: The primary mode of operation shall be asynchronous, allowing the AI to execute long-running commands in the background and continue with other tasks.
- **R1.3**: The system shall provide an optional synchronous execution mode for users who prefer a simpler, blocking workflow.

## 2. Configuration

- **R2.1**: The CLI tool to be adapted must be specified via a TOML configuration file.
- **R2.2**: The path to this configuration file must be provided as a required command-line argument to `ahma_mcp` at startup.
- **R2.3**: The minimal configuration required to adapt a tool shall be the name of the command itself (e.g., `command = "cargo"`).

## 3. Dynamic Tool Discovery and Schema Generation

- **R3.1**: At startup, the system shall inspect the target CLI tool to discover its available subcommands, options, and flags (e.g., by parsing the output of `tool --help`).
- **R3.2**: Based on the discovered information, the system shall dynamically generate a single, unified JSON schema for the MCP tool.
- **R3.3**: This single MCP tool shall represent the entire command-line application. Subcommands (e.g., `build`, `test` for `cargo`) shall be exposed as parameters or an enum within this unified tool schema.
- **R3.4**: The generated schema must accurately reflect all available options for each subcommand, ensuring the AI has a precise interface for the specific version of the tool installed on the host system.

## 4. Asynchronous Operation and AI Interaction

- **R4.1**: In asynchronous mode, invoking a tool shall immediately return an operation ID to the AI.
- **R4.2**: The system shall automatically push the results of a completed operation back to the AI.
- **R4.3**: To encourage concurrent thinking, the system shall provide "tool hints" to the AI after initiating an asynchronous operation. These hints should suggest productive tasks the AI can perform while waiting for the result.

## 5. Configuration Management and Customization

- **R5.1**: The system shall support customization of its behavior through the tool's TOML configuration file.
- **R5.2**: Users shall be able to override the default, automatically generated "tool hints" on a per-command basis within the TOML file.
- **R5.3**: The system shall have permission to automatically update the TOML configuration file to reflect the currently discovered tool structure.
- **R5.4**: These automatic updates shall be added as commented-out examples, providing the user with an always-current template for customization and illustrating the current default behavior if no customization is applied.
- **R5.5**: The system must gracefully handle potential conflicts between its automatic updates and manual user edits to the configuration file. It must provide clear error messages and, where possible, suggest a resolution. For example, it should detect if a user has uncommented and modified a section that the system also wants to update and warn the user accordingly.
