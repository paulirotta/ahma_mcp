use crate::cli_parser::{CliOption, CliStructure};
use crate::config::Config;
use anyhow::Result;
use serde_json::{Value, json};

/// Generates MCP tool schemas from parsed CLI structures.
#[derive(Debug)]
pub struct McpSchemaGenerator;

impl McpSchemaGenerator {
    /// Create a new MCP schema generator.
    pub fn new() -> Self {
        McpSchemaGenerator
    }

    /// Generate a complete MCP tool schema from a CLI structure and config.
    pub fn generate_tool_schema(&self, structure: &CliStructure, config: &Config) -> Result<Value> {
        let tool_name = &structure.tool_name;
        let description = format!("Execute {} commands with async support", tool_name);

        let mut properties = json!({});
        let mut required = Vec::<String>::new();

        // Add subcommand parameter if there are subcommands
        if !structure.subcommands.is_empty() {
            properties["subcommand"] = json!({
                "type": "string",
                "description": format!("The {} subcommand to execute", tool_name),
                "enum": structure.subcommands.iter().map(|cmd| &cmd.name).collect::<Vec<_>>()
            });
            required.push("subcommand".to_string());
        }

        // Add global options as parameters
        for option in &structure.global_options {
            if let Some(param) = self.option_to_parameter(option)? {
                let param_name = self.get_option_parameter_name(option);
                properties[param_name] = param;
            }
        }

        // Add common parameters for all tools
        properties["working_directory"] = json!({
            "type": "string",
            "description": "Working directory for the command execution"
        });
        required.push("working_directory".to_string());

        // Add async control parameters
        properties["enable_async_notification"] = json!({
            "type": "boolean",
            "description": "Enable async callback notifications for operation progress",
            "default": false
        });

        properties["operation_id"] = json!({
            "type": "string",
            "description": "Optional operation ID to assign (if omitted, one is generated)"
        });

        // Add timeout parameter from config
        properties["timeout_seconds"] = json!({
            "type": "integer",
            "description": format!("Operation timeout in seconds (default: {})", config.get_timeout_seconds()),
            "default": config.get_timeout_seconds()
        });

        // Add raw arguments parameter for flexibility
        properties["args"] = json!({
            "type": "array",
            "items": { "type": "string" },
            "description": format!("Additional raw arguments to pass to {}", tool_name)
        });

        // Create the complete tool schema
        Ok(json!({
            "name": tool_name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        }))
    }

    /// Convert a CLI option to an MCP parameter schema.
    fn option_to_parameter(&self, option: &CliOption) -> Result<Option<Value>> {
        // Skip help and version flags as they're not useful in MCP context
        if let Some(ref long) = option.long
            && (long == "help" || long == "version")
        {
            return Ok(None);
        }

        let param_type = if option.takes_value {
            "string"
        } else {
            "boolean"
        };

        let mut param = json!({
            "type": param_type,
            "description": option.description
        });

        // Add default for boolean flags
        if !option.takes_value {
            param["default"] = json!(false);
        }

        // Handle multiple values
        if option.multiple {
            param = json!({
                "type": "array",
                "items": { "type": param_type },
                "description": option.description
            });
        }

        Ok(Some(param))
    }

    /// Get the parameter name for a CLI option in the MCP schema.
    fn get_option_parameter_name(&self, option: &CliOption) -> String {
        // Prefer long name, fallback to short name
        if let Some(ref long) = option.long {
            long.clone()
        } else if let Some(short) = option.short {
            short.to_string()
        } else {
            "unknown_option".to_string()
        }
    }

    /// Generate tool hints for async operations.
    pub fn generate_tool_hints(&self, structure: &CliStructure, config: &Config) -> Value {
        let tool_name = &structure.tool_name;

        let mut hints = json!({
            "default": format!("While {} is running, consider planning next steps or reviewing related code", tool_name)
        });

        // Add subcommand-specific hints from config
        if let Some(config_hints) = &config.hints {
            if let Some(default_hint) = &config_hints.default {
                hints["default"] = json!(default_hint);
            }

            if let Some(build_hint) = &config_hints.build {
                hints["build"] = json!(build_hint);
            }

            if let Some(test_hint) = &config_hints.test {
                hints["test"] = json!(test_hint);
            }

            if let Some(custom_hints) = &config_hints.custom {
                for (cmd, hint) in custom_hints {
                    hints[cmd] = json!(hint);
                }
            }
        }

        hints
    }

    /// Generate the complete MCP server tools manifest.
    pub fn generate_tools_manifest(&self, tools: &[(CliStructure, Config)]) -> Result<Value> {
        let mut tool_schemas = Vec::new();

        for (structure, config) in tools {
            if config.is_enabled() {
                let schema = self.generate_tool_schema(structure, config)?;
                tool_schemas.push(schema);
            }
        }

        Ok(json!({
            "tools": tool_schemas
        }))
    }
}

impl Default for McpSchemaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_parser::{CliOption, CliStructure, CliSubcommand};
    use crate::config::{Config, ToolHints};
    use std::collections::HashMap;

    fn create_test_cli_structure() -> CliStructure {
        let mut structure = CliStructure::new("git".to_string());

        structure.global_options.push(CliOption {
            short: Some('v'),
            long: Some("verbose".to_string()),
            description: "Enable verbose output".to_string(),
            takes_value: false,
            multiple: false,
        });

        structure.global_options.push(CliOption {
            short: Some('C'),
            long: None,
            description: "Run as if git was started in <path>".to_string(),
            takes_value: true,
            multiple: false,
        });

        structure.global_options.push(CliOption {
            short: None,
            long: Some("help".to_string()),
            description: "Show help".to_string(),
            takes_value: false,
            multiple: false,
        });

        structure.subcommands.push(CliSubcommand {
            name: "add".to_string(),
            description: "Add file contents to the index".to_string(),
            options: Vec::new(),
        });

        structure.subcommands.push(CliSubcommand {
            name: "commit".to_string(),
            description: "Record changes to the repository".to_string(),
            options: Vec::new(),
        });

        structure.has_help = true;
        structure.has_version = false;

        structure
    }

    fn create_test_config() -> Config {
        let mut hints = ToolHints {
            build: None,
            test: None,
            default: Some("Consider reviewing code while git operations run".to_string()),
            custom: Some(HashMap::new()),
        };

        hints.custom.as_mut().unwrap().insert(
            "commit".to_string(),
            "Review your changes while commit is being prepared".to_string(),
        );

        Config {
            tool_name: "git".to_string(),
            command: Some("git".to_string()),
            hints: Some(hints),
            overrides: None,
            enabled: Some(true),
            timeout_seconds: Some(300),
            verbose: Some(false),
        }
    }

    #[test]
    fn test_generate_tool_schema_with_subcommands() {
        let generator = McpSchemaGenerator::new();
        let structure = create_test_cli_structure();
        let config = create_test_config();

        let schema = generator.generate_tool_schema(&structure, &config).unwrap();

        // Check basic structure
        assert_eq!(schema["name"], "git");
        assert!(schema["description"].as_str().unwrap().contains("git"));

        let input_schema = &schema["inputSchema"];
        let properties = &input_schema["properties"];
        let required = &input_schema["required"];

        // Should have subcommand parameter
        assert!(properties["subcommand"].is_object());
        assert!(required.as_array().unwrap().contains(&json!("subcommand")));

        // Should have enum with subcommand names
        let subcommand_enum = properties["subcommand"]["enum"].as_array().unwrap();
        assert!(subcommand_enum.contains(&json!("add")));
        assert!(subcommand_enum.contains(&json!("commit")));

        // Should have global options (but not help)
        assert!(properties["verbose"].is_object());
        assert_eq!(properties["verbose"]["type"], "boolean");
        assert!(properties["C"].is_object());
        assert_eq!(properties["C"]["type"], "string");
        assert!(
            properties["help"].is_null() || !properties.as_object().unwrap().contains_key("help")
        );

        // Should have common parameters
        assert!(properties["working_directory"].is_object());
        assert!(properties["enable_async_notification"].is_object());
        assert!(properties["args"].is_object());
        assert_eq!(properties["args"]["type"], "array");
    }

    #[test]
    fn test_generate_tool_schema_without_subcommands() {
        let generator = McpSchemaGenerator::new();
        let mut structure = CliStructure::new("echo".to_string());

        structure.global_options.push(CliOption {
            short: Some('n'),
            long: None,
            description: "Do not output trailing newline".to_string(),
            takes_value: false,
            multiple: false,
        });

        let config = Config::default();

        let schema = generator.generate_tool_schema(&structure, &config).unwrap();

        let input_schema = &schema["inputSchema"];
        let properties = &input_schema["properties"];
        let required = &input_schema["required"];

        // Should NOT have subcommand parameter
        assert!(
            properties["subcommand"].is_null()
                || !properties.as_object().unwrap().contains_key("subcommand")
        );
        assert!(!required.as_array().unwrap().contains(&json!("subcommand")));

        // Should still have common parameters
        assert!(properties["working_directory"].is_object());
        assert!(
            required
                .as_array()
                .unwrap()
                .contains(&json!("working_directory"))
        );
    }

    #[test]
    fn test_option_to_parameter() {
        let generator = McpSchemaGenerator::new();

        // Boolean option
        let bool_option = CliOption {
            short: Some('v'),
            long: Some("verbose".to_string()),
            description: "Enable verbose output".to_string(),
            takes_value: false,
            multiple: false,
        };

        let param = generator
            .option_to_parameter(&bool_option)
            .unwrap()
            .unwrap();
        assert_eq!(param["type"], "boolean");
        assert_eq!(param["default"], false);

        // String option
        let string_option = CliOption {
            short: Some('f'),
            long: Some("file".to_string()),
            description: "Input file path".to_string(),
            takes_value: true,
            multiple: false,
        };

        let param = generator
            .option_to_parameter(&string_option)
            .unwrap()
            .unwrap();
        assert_eq!(param["type"], "string");
        assert!(param["default"].is_null());

        // Multiple values option
        let multi_option = CliOption {
            short: None,
            long: Some("include".to_string()),
            description: "Include multiple patterns".to_string(),
            takes_value: true,
            multiple: true,
        };

        let param = generator
            .option_to_parameter(&multi_option)
            .unwrap()
            .unwrap();
        assert_eq!(param["type"], "array");
        assert_eq!(param["items"]["type"], "string");

        // Help option should be filtered out
        let help_option = CliOption {
            short: None,
            long: Some("help".to_string()),
            description: "Show help".to_string(),
            takes_value: false,
            multiple: false,
        };

        let param = generator.option_to_parameter(&help_option).unwrap();
        assert!(param.is_none());
    }

    #[test]
    fn test_generate_tool_hints() {
        let generator = McpSchemaGenerator::new();
        let structure = create_test_cli_structure();
        let config = create_test_config();

        let hints = generator.generate_tool_hints(&structure, &config);

        assert_eq!(
            hints["default"],
            "Consider reviewing code while git operations run"
        );
        assert_eq!(
            hints["commit"],
            "Review your changes while commit is being prepared"
        );
    }

    #[test]
    fn test_generate_tools_manifest() {
        let generator = McpSchemaGenerator::new();
        let structure1 = create_test_cli_structure();
        let config1 = create_test_config();

        let structure2 = CliStructure::new("disabled_tool".to_string());
        let config2 = Config {
            enabled: Some(false),
            ..Default::default()
        };

        let tools = vec![(structure1, config1), (structure2, config2)];

        let manifest = generator.generate_tools_manifest(&tools).unwrap();

        let tools_array = manifest["tools"].as_array().unwrap();
        assert_eq!(tools_array.len(), 1); // Only enabled tool should be included
        assert_eq!(tools_array[0]["name"], "git");
    }

    #[test]
    fn test_get_option_parameter_name() {
        let generator = McpSchemaGenerator::new();

        // Test with long name
        let option1 = CliOption {
            short: Some('v'),
            long: Some("verbose".to_string()),
            description: "Test".to_string(),
            takes_value: false,
            multiple: false,
        };
        assert_eq!(generator.get_option_parameter_name(&option1), "verbose");

        // Test with only short name
        let option2 = CliOption {
            short: Some('x'),
            long: None,
            description: "Test".to_string(),
            takes_value: false,
            multiple: false,
        };
        assert_eq!(generator.get_option_parameter_name(&option2), "x");

        // Test with neither (edge case)
        let option3 = CliOption {
            short: None,
            long: None,
            description: "Test".to_string(),
            takes_value: false,
            multiple: false,
        };
        assert_eq!(
            generator.get_option_parameter_name(&option3),
            "unknown_option"
        );
    }
}
