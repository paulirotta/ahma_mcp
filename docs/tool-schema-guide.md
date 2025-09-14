# Tool Schema Guide

This guide explains how to define and validate tool schemas for Ahma MCP.

## The `spec.md` and `plan.md` Workflow

All new tools or modifications to existing tools must follow the spec-driven development workflow. This means you must have a `spec.md` and a `plan.md` file.

1. **`spec.md`**: Defines the *what* and *why* of the tool. It should describe the user-facing functionality and the problem it solves.
2. **`plan.md`**: Outlines the *how*. This includes the technical implementation details, such as the MTDF schema, validation rules, and any changes to the server logic.

This process ensures that all tool development is well-documented and reviewed before implementation.

## MTDF Structure

The MCP Tool Definition Format (MTDF) is a JSON-based schema that describes a tool's interface. Each tool is defined in a `.json` file in the `tools/` directory.

### Core Concepts

* **Dynamic Loading**: Tools are loaded and validated at runtime. No server recompilation is needed.
* **Schema Validation**: The `MtdfValidator` ensures all definitions are correct and secure.
* **Centralized Guidance**: AI hints and usage instructions are managed in `tool_guidance.json` to ensure consistency.

### Basic MTDF Example

```json
{
  "name": "my_tool",
  "description": "A brief description of what this tool does.",
  "command": "my_tool_executable",
  "enabled": true,
  "timeout_seconds": 300,
  "subcommand": [
    {
      "name": "run",
      "description": "Executes the primary function of the tool.",
      "synchronous": false,
      "guidance_key": "async_behavior",
      "options": [
        {
          "name": "config",
          "type": "string",
          "description": "Path to the configuration file.",
          "format": "path",
          "required": true
        }
      ]
    }
  ]
}
```

### Key Fields

* `name`: The base name of the tool.
* `command`: The executable to run.
* `subcommand`: An array of subcommands. MTDF supports recursive nesting for complex tools (e.g., `cargo nextest run`).
* `synchronous`: A boolean indicating if the tool returns immediately (`true`) or runs as a background operation (`false`).
* `guidance_key`: A key referencing a block in `tool_guidance.json`. This is the preferred way to provide AI hints.
* `options`: An array of command-line flags.
* `positional_args`: An array of positional arguments.
* `format: "path"`: **CRITICAL** for security. Any option or argument that accepts a file path **must** include this format specifier for validation.

## Development Workflow

1. **Write your `spec.md` and `plan.md`**. Get them reviewed and approved.
2. **Create or modify the tool's `.json` file** in the `tools/` directory according to your plan.
3. **Run the server**. The `MtdfValidator` will automatically validate your schema upon loading.
4. **Check the logs** for any validation errors. The errors are detailed and will point to the exact field that has an issue.
5. **Fix any errors** and restart the server to re-validate.

## Validation

The `MtdfValidator` performs several checks:

* **Syntax Validation**: Ensures the JSON is well-formed.
* **Semantic Validation**: Checks for logical consistency (e.g., `guidance_key` exists).
* **Security Validation**: Enforces that path parameters are correctly marked.

If validation fails, the tool will not be loaded, and a detailed error will be logged.

For a complete reference of the MTDF schema, see `mtdf-schema.json`.
