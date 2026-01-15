# AHMA Tool Configurations

This directory contains tool configuration files for the AHMA MCP (Model Context Protocol) server. These configurations define how AI agents can interact with various command-line tools in a safe and structured way.

## Available Tools

- **sandboxed_shell.json** - Active shell execution tool (enabled by default)

## Example Tool Configurations

Additional tool configurations are available as examples in `ahma_core/examples/configs/`. These can be copied, customized, and validated before deployment:

- **cargo.json** - Rust build tool and package manager
- **file_tools.json** - Unix file operations (ls, cp, mv, rm, grep, etc.)
- **gh.json** - GitHub CLI wrapper
- **git.json** - Git version control client
- **gradlew.json** - Android Gradle wrapper
- **python.json** - Python interpreter

## Usage

### 1. Copy an Example Configuration

```bash
# Copy an example configuration to .ahma directory
cp ahma_core/examples/configs/cargo.json .ahma/

# Or copy all examples
cp ahma_core/examples/configs/*.json .ahma/
```

### 2. Customize the Configuration

Edit the copied JSON file to match your requirements:

```bash
# Edit with your preferred editor
code .ahma/cargo.json
# or
vim .ahma/cargo.json
```

Key fields to customize:
- `enabled`: Set to `true` to activate the tool
- `timeout_seconds`: Adjust based on expected operation duration
- `subcommand`: Add, remove, or modify available subcommands
- `options`: Customize available options for each subcommand

### 3. Validate Your Configuration

Run the validation tool to ensure your configuration is correct:

```bash
# Validate a specific configuration using the example runners
cargo run --example cargo_tool
cargo run --example file_tools
cargo run --example gh_tool
cargo run --example git_tool
cargo run --example gradlew_tool
cargo run --example python_tool

# Or run schema validation tests
cargo test --test tool_config_schema_validation_test

# Or run all tests including execution tests
cargo nextest run --package ahma_core --test tool_config_schema_validation_test
cargo nextest run --package ahma_core --test tool_examples_execution_test
```

### 4. Verify Your Configuration Works

After copying and enabling a configuration in `.ahma/`, restart the AHMA MCP server to load the new tool:

```bash
# The server will automatically discover and load enabled configurations from .ahma/
ahma_mcp --tools-dir .ahma
```

## Configuration Format

All tool configurations follow the MCP Tool Definition Format (MTDF) schema. Here's a minimal example:

```json
{
    "name": "mytool",
    "description": "Description of what the tool does",
    "command": "command-to-execute",
    "enabled": true,
    "timeout_seconds": 300,
    "subcommand": [
        {
            "name": "subcommand_name",
            "description": "What this subcommand does",
            "options": [
                {
                    "name": "option-name",
                    "type": "string",
                    "description": "What this option does",
                    "required": false
                }
            ],
            "synchronous": true
        }
    ]
}
```

## Validation Tools

### Command-Line Validation

```bash
# Validate all example configs
cargo nextest run -p ahma_core tool_config_schema_validation

# Run a specific example to see detailed output
cargo run --example cargo_tool

# Check if configuration is parseable
jq . .ahma/cargo.json
```

### Programmatic Validation

Use the `MtdfValidator` from `ahma_core`:

```rust
use ahma_core::schema_validation::MtdfValidator;
use std::path::Path;

let validator = MtdfValidator::new();
let config_path = Path::new(".ahma/mytool.json");
let content = std::fs::read_to_string(config_path)?;

match validator.validate_tool_config(config_path, &content) {
    Ok(config) => println!("✅ Valid configuration"),
    Err(errors) => {
        eprintln!("❌ Validation errors:");
        for error in errors {
            eprintln!("  - {}: {}", error.field_path, error.message);
        }
    }
}
```

## Security Considerations

- **Path Security**: All file paths are automatically validated and scoped to the current working directory
- **Sandbox Mode**: Commands run in isolated environments with restricted permissions
- **Timeout Protection**: All operations have configurable timeouts to prevent hanging
- **Command Whitelisting**: Only explicitly configured commands and subcommands are available

## Troubleshooting

### Configuration Not Loading

1. Ensure the JSON file is valid: `jq . .ahma/mytool.json`
2. Check that `"enabled": true` is set
3. Verify file permissions: `ls -la .ahma/`
4. Check server logs for parsing errors

### Validation Fails

1. Run the corresponding example: `cargo run --example mytool`
2. Check for schema violations in the error output
3. Compare with working examples in `ahma_core/examples/configs/`
4. Verify all required fields are present: `name`, `description`, `command`, `enabled`

### Tool Not Available in AI

1. Restart the AHMA MCP server
2. Verify tool is enabled: `grep enabled .ahma/mytool.json`
3. Check server initialization logs
4. Ensure the underlying command is installed: `which command-name`

## Schema Documentation

Full MTDF schema documentation is available at:
- `docs/mtdf-schema.json` - JSON Schema definition
- `ahma_core/docs/mtdf-schema.json` - Core library schema

## Contributing

To add a new tool configuration:

1. Create the JSON file in `ahma_core/examples/configs/`
2. Add a corresponding example in `ahma_core/examples/toolname.rs`
3. Add tests in `ahma_core/tests/tool_config_schema_validation_test.rs`
4. Add execution tests in `ahma_core/tests/tool_examples_execution_test.rs`
5. Update `ahma_core/Cargo.toml` with example declaration
6. Run all tests: `cargo nextest run --workspace`

## License

Tool configurations in this directory follow the project's dual MIT/Apache-2.0 license.
