//! # Tool Configuration Management
//!
//! This module defines the data structures and logic for managing the configuration of
//! command-line tools. All tool configurations are loaded from `.toml` files located in
//! the `tools/` directory. This approach allows for easy extension and modification of
//! supported tools without altering the core server code.
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
//! - The `load_from_file` function reads a specified TOML file and deserializes it into a
//!   `Config` struct.
//! - The `load_tool_config` helper function simplifies loading by constructing the path
//!   to a tool's configuration file within the `tools/` directory.
//!
//! ## Key Features
//!
//! - **Declarative Tool Definition**: Tools are defined entirely through TOML, making the
//!   system highly modular and easy to maintain.
//! - **Hierarchical Configuration**: Settings can be applied globally (in `Config`), per
//!   operation type (in `ToolHints`), or per specific subcommand (in `CommandOverride`),
//!   providing a flexible and powerful configuration cascade.
//! - **AI Guidance**: The `ToolHints` system is a key feature for improving the performance
//!   of AI agents using the tools, providing them with contextual advice.
//! - **Dynamic Behavior**: The server's behavior, such as whether a command runs
//!   synchronously or asynchronously, can be controlled directly from the configuration files.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::adapter::ExecutionMode;

/// Represents the complete configuration for a command-line tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub subcommand: Vec<SubcommandConfig>,
    pub input_schema: Value,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: ExecutionMode,
    pub timeout: Option<u64>,
    #[serde(default)]
    pub hints: ToolHints,
    #[serde(default)]
    pub enabled: bool,
}

/// Configuration for a subcommand within a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubcommandConfig {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub options: Vec<OptionConfig>,
}

/// Configuration for an option within a subcommand
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub option_type: String,
    pub description: String,
}

fn default_execution_mode() -> ExecutionMode {
    ExecutionMode::AsyncResultPush
}

/// A collection of hints for a tool.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolHints {
    /// Hints for specific operations like "build" or "test".
    #[serde(flatten)]
    pub operation_hints: HashMap<String, String>,
    /// If true, treat any output on stderr as a failure.
    pub treat_stderr_as_error: Option<bool>,
}

/// Load all tool configurations from the `tools` directory.
pub fn load_tool_configs() -> Result<HashMap<String, ToolConfig>> {
    let mut configs = HashMap::new();
    let tools_dir = PathBuf::from("tools");

    if !tools_dir.exists() {
        return Ok(configs);
    }

    for entry in fs::read_dir(tools_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            let config: ToolConfig = toml::from_str(&contents).with_context(|| {
                format!(
                    "Failed to parse config file: {}. Content:\n{}",
                    path.display(),
                    contents
                )
            })?;
            configs.insert(config.name.clone(), config);
        }
    }

    Ok(configs)
}
