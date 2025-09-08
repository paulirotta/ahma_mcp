//! # Tool Configuration Management
//!
//! This module defines the data structures and logic for managing the configuration of
//! command-line tools. All tool configurations are loaded from `.json` files
//! located in the `.ahma/tools/` directory. This approach allows for easy extension and
//! modification of supported tools without altering the core server code.
//!
//! ## Core Data Structures
//!
//! - **`Config`**: The main struct representing the complete configuration for a single
//!   tool. It includes the tool's name, the actual command to execute, and whether the
//!   tool is enabled. It also contains nested structures for more granular control.
//!
//! - **`ToolHints`**: A collection of strings intended to provide guidance to an AI agent
//!   on how to use the tool effectively. It includes hints for specific operations like
//!   `build` and `test`, as well as custom hints for any subcommand.
//!
//! - **`CommandOverride`**: Allows for overriding default behaviors for specific subcommands.
//!   For example, a `test` subcommand could be given a longer timeout or be forced to run
//!   asynchronously, even if the tool's default is synchronous.
//!
//! ## Configuration Loading
//!
//! - The `load_from_file` function reads a specified JSON file and deserializes it
//!   into a `Config` struct.
//! - The `load_tool_config` helper function simplifies loading by constructing the path
//!   to a tool's configuration file within the `.ahma/tools/` directory.
//!
//! ## Key Features
//!
//! - **Declarative Tool Definition**: Tools are defined entirely through JSON,
//!   making the system highly modular and easy to maintain.
//! - **Hierarchical Configuration**: Settings can be applied globally (in `Config`), per
//!   operation type (in `ToolHints`), or per specific subcommand (in `CommandOverride`),
//!   providing a flexible and powerful configuration cascade.
//! - **AI Guidance**: The `ToolHints` system is a key feature for improving the performance
//!   of AI agents using the tools, providing them with contextual advice.
//! - **Dynamic Behavior**: The server's behavior, such as whether a command runs
//!   synchronously or asynchronously, can be controlled directly from the configuration files.

use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents the complete configuration for a command-line tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<Vec<SubcommandConfig>>,
    /// Generated input schema (optional - auto-generated from subcommands)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    /// Default timeout for operations in seconds
    pub timeout_seconds: Option<u64>,
    /// Default synchronous behavior for all subcommands (can be overridden per subcommand)
    pub synchronous: Option<bool>,
    #[serde(default)]
    pub hints: ToolHints,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Key to look up guidance in tool_guidance.json
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance_key: Option<String>,
}

/// Configuration for a subcommand within a tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubcommandConfig {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<OptionConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positional_args: Option<Vec<OptionConfig>>,
    /// If true, this subcommand runs synchronously instead of async
    pub synchronous: Option<bool>,
    /// Override timeout for this specific subcommand
    pub timeout_seconds: Option<u64>,
    /// Whether this subcommand is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Key to look up guidance in tool_guidance.json
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance_key: Option<String>,
    /// Nested subcommands for recursive command structures
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<Vec<SubcommandConfig>>,
}

/// Configuration for an option within a subcommand
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(rename = "type")]
    pub option_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    /// If true, this option is treated as a positional argument rather than a flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positional: Option<bool>,
    /// If true, multi-line or special character values will be written to a temporary file
    /// and the file path will be passed as the argument instead of the raw value.
    /// The option name will be automatically converted to use the file flag variant
    /// (e.g., for git commit: "message" with file_arg=true becomes "-F" instead of "-m")
    #[serde(default)]
    pub file_arg: Option<bool>,
    /// When file_arg is true, this specifies the flag to use for file-based input
    /// (e.g., "-F" for git commit). If not specified, defaults to the normal flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_flag: Option<String>,
}

fn default_enabled() -> bool {
    true
}

/// A collection of hints for AI clients using this tool.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct ToolHints {
    /// Default hint for any operation with this tool
    pub default: Option<String>,
    /// Hints for specific operations like "build" or "test"
    #[serde(flatten)]
    pub operation_hints: HashMap<String, String>,
}

/// Load all tool configurations from the `tools` directory.
pub fn load_tool_configs(tools_dir: &Path) -> Result<HashMap<String, ToolConfig>> {
    use crate::schema_validation::MtdfValidator;

    let mut configs = HashMap::new();
    let validator = MtdfValidator::new();

    if !tools_dir.exists() {
        return Ok(configs);
    }

    // Track hardcoded tools to detect conflicts
    let hardcoded_tools = ["await", "status", "cancel"];
    let mut detected_conflicts = Vec::new();

    for entry in fs::read_dir(tools_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;

            // First validate with our schema validator
            match validator.validate_tool_config(&path, &contents) {
                Ok(config) => {
                    // CRITICAL: Check for hardcoded tool conflicts
                    if hardcoded_tools.contains(&config.name.as_str()) {
                        detected_conflicts.push((config.name.clone(), path.clone()));
                        eprintln!(
                            "⚠️  CRITICAL: JSON configuration found for hardcoded tool '{}'",
                            config.name
                        );
                        eprintln!("   File: {}", path.display());
                        eprintln!(
                            "   This will cause the tool to be processed as a command-line tool instead of using hardcoded MCP logic!"
                        );
                        eprintln!(
                            "   Please remove this JSON file or rename the tool to avoid conflicts.\n"
                        );
                    }

                    if config.enabled {
                        configs.insert(config.name.clone(), config);
                    }
                }
                Err(validation_errors) => {
                    let error_report = validator.format_errors(&validation_errors, &path);
                    eprintln!("⚠️  Schema validation failed for tool configuration:");
                    eprintln!("{}", error_report);

                    // Try to fallback to direct parsing for backward compatibility
                    match serde_json::from_str::<ToolConfig>(&contents) {
                        Ok(config) => {
                            eprintln!(
                                "📝 Configuration parsed successfully despite schema validation errors."
                            );
                            eprintln!(
                                "💡 Consider updating the configuration to match the schema for better reliability.\n"
                            );

                            if config.enabled {
                                configs.insert(config.name.clone(), config);
                            }
                        }
                        Err(parse_error) => {
                            return Err(anyhow::anyhow!(
                                "Failed to parse tool configuration {}: Schema validation failed with {} errors, and JSON parsing also failed: {}",
                                path.display(),
                                validation_errors.len(),
                                parse_error
                            ));
                        }
                    }
                }
            }
        }
    }

    // Return error if hardcoded tool conflicts were detected
    if !detected_conflicts.is_empty() {
        return Err(anyhow::anyhow!(
            "❌ Tool configuration conflicts detected!\n\n\
            Found JSON files for hardcoded tools: {}\n\n\
            These tools are hardcoded in the MCP service and should NOT have JSON configuration files.\n\
            Please move these files to tools_backup/ or rename them to resolve the conflict.\n\n\
            The affected files are:\n{}\n\n\
            For more information, see docs/completed-issues/tool-configuration-conflicts.md",
            detected_conflicts
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            detected_conflicts
                .iter()
                .map(|(_, path)| format!("  - {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    Ok(configs)
}
