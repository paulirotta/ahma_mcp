# Tool Schema Guide

This document provides guidance for developers creating new tools for the AHMA MCP server, focusing on JSON schema validation and error handling.

## Overview

The AHMA MCP server uses JSON schemas to validate tool configurations and provide precise error messages when tool definitions don't comply with expected formats. This ensures robust tool loading and clear developer feedback.

## Schema Structure

Each tool must define a JSON schema that describes its configuration format. The schema is used for:

1. **Validation**: Ensuring tool configurations are correctly formatted
2. **Error reporting**: Providing precise feedback about validation failures
3. **Documentation**: Auto-generating tool parameter documentation
4. **IDE support**: Enabling autocompletion and validation in development environments

## Tool Definition Format

### Basic Structure

```json
{
  "name": "tool_name",
  "description": "Brief description of what the tool does",
  "inputSchema": {
    "type": "object",
    "properties": {
      "parameter_name": {
        "type": "string",
        "description": "Description of the parameter",
        "default": "optional_default_value"
      }
    },
    "required": ["parameter_name"],
    "additionalProperties": false
  }
}
```

### Schema Validation Rules

1. **Required Fields**: Every tool must have `name`, `description`, and `inputSchema`
2. **Parameter Types**: Supported types are `string`, `number`, `integer`, `boolean`, `array`, `object`
3. **Descriptions**: All parameters must have meaningful descriptions
4. **Defaults**: Optional parameters should specify default values when appropriate
5. **Additional Properties**: Set to `false` to prevent unexpected parameters

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
        "timeout": {"type": "number"}
      }
    }
  },
  "if": {
    "properties": {"mode": {"const": "advanced"}}
  },
  "then": {
    "required": ["advanced_options"]
  }
}
```

## Error Handling

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
        "items": {"type": "string"},
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
        "items": {"type": "string"},
        "description": "Command-line arguments",
        "default": []
      },
      "env_vars": {
        "type": "object",
        "additionalProperties": {"type": "string"},
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

For questions or suggestions, please refer to the main project documentation or create an issue in the repository.
