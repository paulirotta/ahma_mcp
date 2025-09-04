# MCP Tool Definition Format (MTDF) Guide

This document provides comprehensive guidance for creating tool definitions using the **MCP Tool Definition Format (MTDF)**. MTDF enables dynamic tool integration in Ahma MCP without requiring code changes or server recompilation.

## Overview

Ahma MCP uses the **MCP Tool Definition Format (MTDF)** for defining command-line tools. MTDF is a JSON-based schema that describes:

- Tool commands and subcommands
- Parameter definitions and validation
- Synchronous vs. asynchronous behavior
- AI guidance integration
- Path security and validation

The **MtdfValidator** ensures all tool definitions comply with the MTDF specification and provides detailed error reporting when issues are detected.

## MTDF Structure

Each tool is defined in a single JSON file in the `tools/` directory. The MTDF schema provides:

1. **Dynamic Schema Generation**: Tool schemas are generated at runtime from MTDF definitions
2. **Validation**: The MtdfValidator ensures all tool configurations comply with MTDF specification
3. **Error Reporting**: Detailed validation errors with suggestions for fixes
4. **Guidance Integration**: Centralized guidance system via `tool_guidance.json`
5. **Security**: Path validation and format enforcement

## MTDF Tool Definition Format

### Basic MTDF Structure

```json
{
  "name": "tool_name",
  "description": "Brief description of the tool family",
  "command": "base_command",
  "enabled": true,
  "timeout_seconds": 300,
  "hints": {
    "default": "Guidance for AI while tool executes"
  },
  "subcommand": [
    {
      "name": "subcommand_name",
      "description": "Description of this specific subcommand",
      "synchronous": false,
      "guidance_key": "async_behavior",
      "options": [
        {
          "name": "parameter_name",
          "type": "string",
          "description": "Parameter description",
          "format": "path",
          "required": true
        }
      ],
      "positional_args": [
        {
          "name": "arg_name",
          "type": "string",
          "description": "Positional argument description"
        }
      ]
    }
  ]
}
```

### MTDF Validation Rules

1. **Required Fields**: Every tool must have `name`, `description`, `command`, and `subcommand`
2. **Subcommand Structure**: Each subcommand must have `name` and `description`
3. **Parameter Types**: Supported types are `string`, `integer`, `boolean`, `array`
4. **Path Security**: Parameters representing file paths must include `"format": "path"`
5. **Guidance Integration**: Use `guidance_key` to reference centralized guidance blocks
6. **Synchronous Marking**: Fast operations should include `"synchronous": true`

### Centralized Guidance System

The MTDF integrates with a centralized guidance system via `tool_guidance.json`:

```json
{
  "guidance_blocks": {
    "async_behavior": "**IMPORTANT:** This tool operates asynchronously...",
    "sync_behavior": "This tool runs synchronously and returns results immediately.",
    "blocking_warning": "**WARNING:** This is a blocking tool...",
    "coordination_tool": "**WARNING:** This is a blocking coordination tool. Use ONLY for final project validation when no other productive work remains."
  },
  "templates": {
    "async_full": "**IMPORTANT:** This tool operates asynchronously.
1. **Immediate Response:** Returns operation_id and status 'started'. NOT success.
2. **Final Result:** Result pushed automatically via MCP notification when complete.

**Your Instructions:**
- **DO NOT** wait for the final result.
- **DO** continue with other tasks that don't depend on this operation.
- You **MUST** process the future result notification to know if operation succeeded."
  }
}
```

**Usage in MTDF:**

```json
{
  "name": "subcommand_name",
  "guidance_key": "async_behavior",
  "description": "Brief description without embedded guidance text"
}
```

**Benefits:**

- **Consistency**: All tools using `async_behavior` get identical guidance
- **Maintainability**: Update guidance in one place affects all tools
- **Validation Efficiency**: Tools with `guidance_key` skip embedded guidance validation
- **Backward Compatibility**: Tools without `guidance_key` continue to work with embedded guidance

The server automatically prepends the guidance block to the description when generating the final tool schema.

### Recursive Subcommands

MTDF supports recursive subcommand structures, allowing you to model complex CLI tools with nested subcommands like `cargo nextest run` or `gh cache delete`. This enables precise tool modeling without flattening the command structure.

#### Basic Recursive Structure

```json
{
  "name": "cargo",
  "description": "Rust package manager",
  "command": "cargo",
  "subcommand": [
    {
      "name": "nextest",
      "description": "Next-generation testing framework",
      "subcommand": [
        {
          "name": "run",
          "description": "Run tests with nextest",
          "synchronous": false,
          "guidance_key": "async_behavior",
          "options": [
            {
              "name": "workspace",
              "type": "string",
              "description": "Run tests for all packages in workspace"
            }
          ]
        }
      ]
    }
  ]
}
```

#### Tool Name Generation

Recursive subcommands generate hierarchical tool names using the pattern:
`mcp_{server_name}_{tool_name}_{subcommand}_{nested_subcommand}`

Examples:

- `cargo nextest run` → `mcp_ahma_mcp_cargo_nextest_run`
- `gh cache delete` → `mcp_ahma_mcp_gh_cache_delete`
- `git log --oneline` → `mcp_ahma_mcp_git_log`

#### Benefits of Recursive Structure

1. **Semantic Clarity**: Tool names reflect the actual command structure
2. **Logical Organization**: Related subcommands are grouped naturally
3. **Inheritance**: Parent commands can define default behaviors inherited by nested subcommands
4. **Scalability**: Add new nested subcommands without restructuring existing definitions

#### Inheritance Patterns

Nested subcommands inherit properties from their parent unless explicitly overridden:

```json
{
  "name": "gh",
  "synchronous": true, // All subcommands inherit synchronous behavior
  "subcommand": [
    {
      "name": "cache",
      "subcommand": [
        {
          "name": "delete",
          // Inherits synchronous: true from parent tool
          "description": "Delete GitHub Actions caches"
        },
        {
          "name": "list",
          "synchronous": false, // Override parent setting
          "description": "List GitHub Actions caches"
        }
      ]
    }
  ]
}
```

### Advanced Schema Features

#### Enums for Constrained Values

```json
{
  "type": "string",
  "enum": ["option1", "option2", "option3"],
  "description": "Select from available options"
}
```

#### Array Parameters

```json
{
  "type": "array",
  "items": {
    "type": "string"
  },
  "description": "List of string values",
  "minItems": 1,
  "maxItems": 10
}
```

#### Conditional Requirements

```json
{
  "type": "object",
  "properties": {
    "mode": {
      "type": "string",
      "enum": ["simple", "advanced"]
    },
    "advanced_options": {
      "type": "object",
      "properties": {
        "timeout": { "type": "number" }
      }
    }
  },
  "if": {
    "properties": { "mode": { "const": "advanced" } }
  },
  "then": {
    "required": ["advanced_options"]
  }
}
```

## Error Handling and MtdfValidator

### MtdfValidator Features

The **MtdfValidator** (renamed from SchemaValidator) provides comprehensive MTDF validation:

- **Syntax Validation**: Ensures JSON structure compliance with MTDF specification
- **Semantic Validation**: Checks logical consistency (e.g., path parameters must have `"format": "path"`)
- **Guidance Validation**: Verifies async/sync operations have appropriate guidance
- **Security Validation**: Enforces path security requirements and prevents injection vulnerabilities
- **Performance Optimization**: Tools with `guidance_key` skip embedded guidance validation for faster loading

### Validation Error Format

When a tool configuration fails validation, the system provides structured error messages:

```json
{
  "error": "Schema validation failed",
  "tool": "tool_name",
  "details": [
    {
      "path": "/parameter_name",
      "message": "Missing required parameter 'parameter_name'",
      "expected": "string",
      "received": null
    }
  ]
}
```

### Common Validation Errors

1. **Missing Required Parameters**

   - Message: "Missing required parameter '{name}'"
   - Solution: Add the required parameter to your tool configuration

2. **Invalid Parameter Types**

   - Message: "Expected {type}, received {actual_type}"
   - Solution: Convert the parameter to the expected type

3. **Invalid Enum Values**

   - Message: "Value must be one of: {allowed_values}"
   - Solution: Use one of the specified enum values

4. **Array Constraints**
   - Message: "Array must have between {min} and {max} items"
   - Solution: Adjust array size to meet constraints

## Development Workflow

### 1. Define Your Tool Schema

Create a JSON file in the `/tools/` directory:

```bash
# Example: tools/my_tool.json
{
  "name": "my_tool",
  "description": "Example tool for demonstration",
  "inputSchema": {
    "type": "object",
    "properties": {
      "input_file": {
        "type": "string",
        "description": "Path to input file",
        "pattern": "^[^<>:\"|?*]+$"
      },
      "options": {
        "type": "array",
        "items": {"type": "string"},
        "description": "Additional command-line options",
        "default": []
      }
    },
    "required": ["input_file"],
    "additionalProperties": false
  }
}
```

### 2. Test Your Schema

Use the built-in validation to test your tool:

```rust
// In VS Code MCP environment
// The system will automatically validate your tool when loading
// Check logs for any validation errors

// In CLI mode
cargo run -- --validate-tools
```

### 3. Handle Validation Errors

When the system reports validation errors:

1. **Read the error path**: Shows exactly which parameter failed
2. **Check the expected type**: Ensure your parameter matches
3. **Verify required fields**: Make sure all required parameters are present
4. **Test incrementally**: Add parameters one at a time to isolate issues

### 4. Best Practices

#### Schema Design

- Use descriptive parameter names
- Provide comprehensive descriptions
- Set appropriate constraints (min/max values, patterns)
- Define sensible defaults
- Use enums for limited option sets

#### Error Prevention

- Test with various input combinations
- Validate edge cases
- Document parameter interactions
- Use clear, actionable error messages

#### Performance

- Keep schemas simple and flat when possible
- Avoid deeply nested objects unless necessary
- Use appropriate data types (integer vs number)

## Integration Examples

### CLI Tool Integration

```json
{
  "name": "cargo_build",
  "description": "Build Rust project with cargo",
  "inputSchema": {
    "type": "object",
    "properties": {
      "release": {
        "type": "boolean",
        "description": "Build in release mode",
        "default": false
      },
      "features": {
        "type": "array",
        "items": { "type": "string" },
        "description": "List of features to enable",
        "default": []
      },
      "target": {
        "type": "string",
        "description": "Target triple for cross-compilation",
        "pattern": "^[a-zA-Z0-9_-]+$"
      }
    },
    "additionalProperties": false
  }
}
```

### Python Script Integration

```json
{
  "name": "python_script",
  "description": "Execute Python script with arguments",
  "inputSchema": {
    "type": "object",
    "properties": {
      "script_path": {
        "type": "string",
        "description": "Path to Python script",
        "pattern": "\\.py$"
      },
      "args": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Command-line arguments",
        "default": []
      },
      "env_vars": {
        "type": "object",
        "additionalProperties": { "type": "string" },
        "description": "Environment variables to set",
        "default": {}
      }
    },
    "required": ["script_path"],
    "additionalProperties": false
  }
}
```

## Troubleshooting

### Common Issues

1. **Schema Not Loading**

   - Check file syntax with JSON validator
   - Verify file is in correct `/tools/` directory
   - Ensure proper file permissions

2. **Validation Always Failing**

   - Double-check required vs optional parameters
   - Verify data types match schema
   - Check for typos in parameter names

3. **Performance Issues**
   - Simplify complex nested schemas
   - Avoid excessive regex patterns
   - Consider splitting complex tools

### Debugging Tools

1. **Schema Validation Testing**

   ```bash
   cargo run -- --validate-schema tools/my_tool.json
   ```

2. **Verbose Error Reporting**

   ```bash
   cargo run -- --debug-validation
   ```

3. **Schema Documentation Generation**
   ```bash
   cargo run -- --generate-docs
   ```

## Future Enhancements

The schema system is designed to be extensible. Planned features include:

- **Auto-completion**: IDE integration for parameter suggestions
- **Dynamic validation**: Real-time validation during parameter entry
- **Schema versioning**: Support for tool schema evolution
- **Custom validators**: User-defined validation functions
- **Interactive schema builder**: GUI for creating tool schemas

## Contributing

When adding new schema features:

1. Update this documentation
2. Add comprehensive tests
3. Ensure backward compatibility
4. Update error message templates
5. Consider performance implications

## JSON Schema Reference

The complete MTDF JSON schema is available at [`mtdf-schema.json`](./mtdf-schema.json). This auto-generated schema file:

- Provides precise validation rules for all MTDF properties
- Enables IDE support with autocomplete and validation
- Documents all supported field types and constraints
- Reflects the current recursive subcommand structure

Use this schema file to validate your MTDF tool definitions and for IDE integration.

For questions or suggestions, please refer to the main project documentation or create an issue in the repository.
