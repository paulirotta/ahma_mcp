# Generate Tool Schema

Utility for generating JSON schemas for Ahma MCP tool configurations.

## Overview

`generate_tool_schema` creates JSON schema definitions for tool configuration files, making it easier to validate and document tool definitions.

## Usage

```bash
# Generate schema for all tools
generate_tool_schema --output-dir ./schemas

# Generate schema for a specific tool type
generate_tool_schema --tool-type command --output ./command-schema.json
```

## Output

Generates JSON schema files that can be used for:
- IDE autocomplete and validation
- Documentation generation
- Configuration validation
- Tool development

## License

MIT OR Apache-2.0

