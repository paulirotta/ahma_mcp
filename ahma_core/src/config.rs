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
//!   synchronously, even if the CLI --async flag is set.
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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, path::Path};

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
    /// Force synchronous execution even when --async flag is set (overrides CLI flag)
    /// If true: always runs synchronously, ignoring --async
    /// If false or None: obeys the --async CLI flag (default behavior)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_synchronous: Option<bool>,
    #[serde(default)]
    pub hints: ToolHints,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Key to look up guidance in tool_guidance.json
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance_key: Option<String>,
    /// Optional sequence of tools to execute in order (for composite tools)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<Vec<SequenceStep>>,
    /// Delay in milliseconds between sequence steps (default: SEQUENCE_STEP_DELAY_MS)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_delay_ms: Option<u64>,
    /// Runtime availability probe executed at server startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_check: Option<AvailabilityCheck>,
    /// Installation guidance displayed when the tool is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_instructions: Option<String>,
}

/// Configuration for a subcommand, allowing for nested commands.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubcommandConfig {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<Vec<SubcommandConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<CommandOption>>,
    /// Optional arguments that are not flags, but positional values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positional_args: Option<Vec<PositionalArgsConfig>>,
    /// Override timeout for this specific subcommand
    pub timeout_seconds: Option<u64>,
    /// Force synchronous execution even when --async flag is set (overrides CLI flag)
    /// If true: always runs synchronously, ignoring --async
    /// If false or None: obeys the --async CLI flag (default behavior)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_synchronous: Option<bool>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Key to look up guidance in tool_guidance.json
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance_key: Option<String>,
    /// Optional sequence of tools to execute in order (for composite tools)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<Vec<SequenceStep>>,
    /// Delay in milliseconds between sequence steps (default: SEQUENCE_STEP_DELAY_MS)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_delay_ms: Option<u64>,
    /// Runtime availability probe executed at server startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_check: Option<AvailabilityCheck>,
    /// Installation guidance displayed when the subcommand is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_instructions: Option<String>,
}

/// Configuration for a single command-line option.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandOption {
    pub name: String,
    #[serde(rename = "type")]
    pub option_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<ItemsSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_arg: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_flag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// Type alias for backward compatibility with tests
pub type OptionConfig = CommandOption;

/// Type alias for positional arguments configuration (same as OptionConfig)
pub type PositionalArgsConfig = CommandOption;

/// Schema details for array items.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ItemsSpec {
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Provides hints to an AI agent on how to use a tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ToolHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<HashMap<String, String>>,
}

/// Defines how to probe for tool or subcommand availability at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AvailabilityCheck {
    /// Optional override for the executable to invoke during the check. Defaults to the tool command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Arguments passed to the availability probe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Working directory used for the probe (defaults to project root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Exit codes considered successful (defaults to `[0]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_exit_codes: Option<Vec<i32>>,
    /// If true, do not append derived subcommand arguments when constructing the probe command.
    #[serde(default, skip_serializing_if = "is_false")]
    pub skip_subcommand_args: bool,
}

/// Represents a single step in a tool sequence
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceStep {
    /// Name of the tool to invoke
    pub tool: String,
    /// Subcommand within that tool
    pub subcommand: String,
    /// Arguments to pass to the tool
    #[serde(default)]
    pub args: Map<String, Value>,
    /// Optional description for logging/display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub servers: HashMap<String, ServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerConfig {
    #[serde(rename = "child_process")]
    ChildProcess(ChildProcessConfig),
    #[serde(rename = "http")]
    Http(HttpServerConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildProcessConfig {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    pub url: String,
    pub atlassian_client_id: Option<String>,
    pub atlassian_client_secret: Option<String>,
}

// Helper functions for serde defaults
fn default_enabled() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

pub fn load_mcp_config(config_path: &Path) -> anyhow::Result<McpConfig> {
    if !config_path.exists() {
        return Ok(McpConfig {
            servers: HashMap::new(),
        });
    }

    let contents = std::fs::read_to_string(config_path)?;
    let config: McpConfig = serde_json::from_str(&contents)?;
    Ok(config)
}

/// Load all tool configurations from a directory
///
/// This function scans the specified directory for JSON files and attempts to
/// deserialize each one into a `ToolConfig`. If the directory doesn't exist or
/// is empty, an empty HashMap is returned.
///
/// # Arguments
/// * `tools_dir` - Path to the directory containing tool configuration files
///
/// # Returns
/// * `Result<HashMap<String, ToolConfig>>` - Map of tool name to configuration or error
pub fn load_tool_configs(tools_dir: &Path) -> anyhow::Result<HashMap<String, ToolConfig>> {
    use std::fs;

    // Hardcoded tool names that should not be overridden by user configurations
    const RESERVED_TOOL_NAMES: &[&str] = &["await", "status"];

    // Return empty map if directory doesn't exist
    if !tools_dir.exists() {
        return Ok(HashMap::new());
    }

    let mut configs = HashMap::new();

    // Read directory entries
    for entry in fs::read_dir(tools_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only process .json files
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            match fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str::<ToolConfig>(&contents) {
                    Ok(config) => {
                        // Guard rail: Check for conflicts with hardcoded tool names
                        if RESERVED_TOOL_NAMES.contains(&config.name.as_str()) {
                            anyhow::bail!(
                                "Tool name '{}' conflicts with a hardcoded system tool. Reserved tool names: {:?}. Please rename your tool in {}",
                                config.name,
                                RESERVED_TOOL_NAMES,
                                path.display()
                            );
                        }

                        // Only include enabled tools
                        if config.enabled {
                            configs.insert(config.name.clone(), config);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(configs)
}
