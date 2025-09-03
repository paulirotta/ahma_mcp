# Dynamic Tool Loading Architecture

This document outlines the critical design principle for how `ahma_mcp` discovers and exposes tools to AI agents. Adherence to this principle is mandatory for all future development.

## The Core Principle: Zero-Compilation Tool Integration

The fundamental requirement for `ahma_mcp` is that a user must be able to add, remove, or modify a tool's capabilities **without recompiling the `ahma_mcp` binary.**

The server is designed to be a general-purpose "tool router." It should not have any hardcoded knowledge of the specific tools it manages (e.g., `cargo`, `ls`, `python3`).

## How It Works

1.  **Tool Definition Files:** All tool definitions are stored in `.json` files within the `/tools` directory. These files follow a specific structure, the "MCP Tool Definition Format" (MTDF).
2.  **Runtime Discovery:** At startup, `ahma_mcp` scans the `/tools` directory and parses every `.json` file it finds.
3.  **Dynamic Schema Generation:** For each tool and subcommand defined in the MTDF files, the server **dynamically generates the required JSON schema** for the language model at runtime.
4.  **Dynamic Execution:** When a tool call is received, the server uses the information from the parsed MTDF to construct and execute the command, passing the arguments provided by the model.

## What to Avoid

**Under no circumstances should the `ahma_mcp` source code be modified to add a new tool.**

Specifically, this means:

- **Do not create tool-specific Rust structs** in the `ahma_mcp` codebase for the purpose of generating a schema (e.g., `CargoParams`, `LsParams`).
- **Do not create `match` statements or `if/else` chains** that are keyed on tool names to decide which schema to generate.

The system must remain generic and driven entirely by the content of the `/tools` directory. This ensures that any user can drop in a new `my-cool-tool.json` file and have it immediately available to their AI agent.
