# Product Requirements for Ahma MCP

This document outlines the high-level requirements for `ahma_mcp`, a universal, config-driven adapter for exposing command-line tools to AI agents via the Model Context Protocol (MCP).

## 1. Core Functionality

- **R1.1**: The system shall adapt any command-line tool for use as a set of MCP tools based on a declarative JSON configuration file.
- **R1.2**: The primary mode of operation shall be asynchronous by default, with automatic result push notifications to AI clients when operations complete. Long-running operations immediately return an operation ID and execute in the background without blocking the AI.
- **R1.3**: Individual subcommands can be marked as `synchronous = true` in their JSON configuration for fast operations (e.g., status, version). Synchronous operations block and return results directly without operation IDs or notifications.
- **R1.4**: The system shall use a pre-warmed shell pool to achieve 10x faster command startup times (5-20ms vs 50-200ms), optimizing performance for both synchronous and asynchronous operations.

## 2. Configuration and Tool Definition

- **R2.1**: All tools shall be defined in `.json` files located in a specified `tools/` directory.
- **R2.2**: The system shall scan this directory at startup and load all valid and enabled tool configurations.
- **R2.3**: The `Config` structure (`src/config.rs`) shall define the schema for these files, including the base command, a list of subcommands, and their options.
- **R2.4**: Each subcommand defined in the JSON will be exposed as a distinct MCP tool (e.g., `cargo build` becomes the `cargo_build` tool).

## 3. Dynamic Schema Generation

- **R3.1**: For each subcommand in a tool's configuration, the system shall generate a corresponding MCP tool definition.
- **R3.2**: The tool name shall be a combination of the base command and the subcommand name (e.g., `git_commit`).
- **R3.3**: The `input_schema` for each tool shall be dynamically generated based on the `options` defined in the `Subcommand` struct in the JSON file. This provides a strongly-typed interface for the AI client.
- **R3.4**: The system must support basic data types for options, such as `boolean`, `string`, `integer`, and `array`.

## 4. Asynchronous Operation and AI Interaction

- **R4.1**: In asynchronous mode (default), invoking a tool shall immediately return a unique operation ID and started status to the AI client.
- **R4.2**: The system shall automatically push operation results back to the AI client via MCP progress notifications upon completion, eliminating the need for client polling.
- **R4.3**: Tool descriptions shall include explicit guidance to AI clients about asynchronous behavior, instructing them to continue productive work rather than waiting for results.
- **R4.4**: The system shall strongly discourage the use of "wait" tools, making them available only for final project validation when no other productive work remains.

## 5. Synchronous Operation

- **R5.1**: Fast operations (e.g., status, version) can be marked with `synchronous = true` in their JSON subcommand configuration to execute in blocking mode.
- **R5.2**: Synchronous operations return final results directly with no operation IDs, notifications, or waiting mechanisms.
- **R5.3**: Tool descriptions for synchronous operations shall be clear and simple, with no mention of asynchronous behavior or operation tracking.

## 6. Performance and Reliability

- **R6.1**: The system shall use a high-performance, pre-warmed shell pool (`ShellPoolManager`) to minimize command startup latency, achieving 10x faster startup times (5-20ms vs 50-200ms) for commands executed within a known working directory.
- **R6.2**: Operations shall be isolated to prevent interference between concurrent commands, with proper resource management and cleanup.
- **R6.3**: The system shall provide robust error handling with automatic timeout management, clearly reporting command failures, timeouts, and configuration errors via MCP notifications.
- **R6.4**: Resource usage shall be optimized to handle multiple concurrent operations efficiently, with background health monitoring and automatic cleanup of idle resources.

## 7. AI Client Guidance and User Experience

- **R7.1**: Tool descriptions shall include standardized guidance for AI clients, explicitly stating asynchronous behavior and instructing clients to continue productive work while operations execute in the background.
- **R7.2**: The system shall provide context-aware hints to AI clients about productive parallel work they can perform while waiting for asynchronous operations to complete.
- **R7.3**: "Wait" functionality shall be available but strongly discouraged through tool descriptions, positioned only for final project validation when no other tasks remain.
- **R7.4**: Tool descriptions shall use imperative language and clear formatting to guide AI behavior effectively, reducing confusion and optimizing productivity.

## 8. Extensibility and Maintainability

## 8. JSON Schema Validation and Developer Experience

- **R8.1**: The system shall implement comprehensive JSON schema validation for all tool configurations, ensuring correctness and providing precise error feedback during development.
- **R8.2**: Schema validation errors shall provide structured information including the exact field path, error type, expected vs actual values, and actionable remediation suggestions.
- **R8.3**: The system shall automatically validate all tool configurations during server startup, preventing deployment of invalid configurations.
- **R8.4**: Schema validation shall support advanced features including:
  - Type checking (string, number, boolean, array, object)
  - Required vs optional field validation
  - Enum value constraints
  - Pattern matching for string fields
  - Array size and item type constraints
- **R8.5**: The system shall provide clear developer guidance for creating new tool configurations, including schema documentation, examples, and troubleshooting guides.
- **R8.6**: Schema validation shall work consistently across all deployment scenarios (VS Code MCP, command-line usage, CI/CD environments).

## 9. Extensibility and Maintainability

- **R9.1**: The architecture shall be modular, with a clear separation of concerns between configuration (`config.rs`), schema validation (`schema_validation.rs`), MCP service logic (`mcp_service.rs`), and command execution (`adapter.rs`).
- **R9.2**: Adding new tools or modifying existing ones should be achievable purely by editing JSON files, with automatic schema validation ensuring correctness.
- **R9.3**: The system shall provide comprehensive logging and debugging capabilities to facilitate troubleshooting and performance optimization.
- **R9.4**: Configuration schema shall support rich metadata including timeout overrides, LLM guidance hints, and performance tuning parameters.
- **R9.5**: The system shall support future schema evolution through versioning mechanisms and backward compatibility strategies.
