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
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents the complete configuration for a command-line tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The base command to execute (e.g., "cargo", "git").
    pub command: String,

    /// A list of subcommands to be exposed as individual MCP tools.
    #[serde(default)]
    pub subcommand: Vec<Subcommand>,

    /// Optional: Tool-specific hints for AI guidance.
    pub hints: Option<HashMap<String, String>>,

    /// Whether this tool is enabled. Defaults to true.
    pub enabled: Option<bool>,
}

/// Defines a subcommand to be exposed as a distinct MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subcommand {
    /// The name of the subcommand (e.g., "build", "test").
    pub name: String,

    /// A description of what the subcommand does.
    pub description: String,

    /// A list of options (flags) that the subcommand accepts.
    #[serde(default)]
    pub options: Vec<CliOption>,

    /// If true, this subcommand will always run synchronously.
    #[serde(default)]
    pub synchronous: Option<bool>,
}

/// Defines a single command-line option for a subcommand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliOption {
    /// The name of the option (e.g., "release", "jobs").
    pub name: String,

    /// The data type of the option's value.
    #[serde(rename = "type")]
    pub type_: String, // "boolean", "string", "integer", "array"

    /// A description of what the option does.
    pub description: String,
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&contents).with_context(|| {
            format!(
                "Failed to parse config file: {}. Content:\n{}",
                path.display(),
                contents
            )
        })?;

        Ok(config)
    }

    /// Load configuration for a specific tool from the tools directory.
    pub fn load_tool_config(tool_name: &str) -> Result<Self> {
        let config_path = PathBuf::from("tools").join(format!("{}.toml", tool_name));
        Self::load_from_file(&config_path)
    }

    /// Check if the tool is enabled (default: true).
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}
