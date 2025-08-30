use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents the complete configuration for a command-line tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The name of the tool (e.g., "cargo", "git", "npm")
    pub tool_name: String,

    /// The command to execute (defaults to tool_name if not specified)
    pub command: Option<String>,

    /// Tool-specific hints for AI guidance during operations
    pub hints: Option<ToolHints>,

    /// Override configurations for specific subcommands
    pub overrides: Option<HashMap<String, CommandOverride>>,

    /// Whether the entire tool should run synchronously by default (overridden by CLI flag or per-command overrides)
    pub synchronous: Option<bool>,

    /// Whether this tool should be available by default
    pub enabled: Option<bool>,

    /// Timeout in seconds for operations (default: 300)
    pub timeout_seconds: Option<u64>,

    /// Whether to enable verbose logging for this tool
    pub verbose: Option<bool>,
}

/// AI guidance hints for different phases of tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHints {
    /// Primary hint shown when AI considers using the tool
    pub primary: Option<String>,

    /// Usage examples and common operations
    pub usage: Option<String>,

    /// What the AI should think about during long operations
    pub wait_hint: Option<String>,

    /// Suggestions for what to think about during build operations
    pub build: Option<String>,

    /// Suggestions for what to think about during test operations
    pub test: Option<String>,

    /// Default suggestion for any long-running operation
    pub default: Option<String>,

    /// Custom hints for specific subcommands
    pub custom: Option<HashMap<String, String>>,

    /// Parameter-specific hints for better AI understanding
    pub parameters: Option<HashMap<String, String>>,
}

/// Override configuration for specific subcommands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOverride {
    /// Custom timeout for this specific command
    pub timeout_seconds: Option<u64>,

    /// Whether this command should run synchronously by default
    pub synchronous: Option<bool>,

    /// Custom hint for this specific command  
    pub hint: Option<String>,

    /// AI guidance hints for this subcommand
    pub hints: Option<ToolHints>,

    /// Additional arguments to always include
    pub default_args: Option<Vec<String>>,
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Load configuration for a specific tool from the tools directory.
    pub fn load_tool_config(tool_name: &str) -> Result<Self> {
        let config_path = PathBuf::from("tools").join(format!("{}.toml", tool_name));
        Self::load_from_file(&config_path)
    }

    /// Get the actual command to execute (tool_name if command is None).
    pub fn get_command(&self) -> &str {
        self.command.as_ref().unwrap_or(&self.tool_name)
    }

    /// Check if the tool is enabled (default: true).
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    /// Get the timeout in seconds (default: 300).
    pub fn get_timeout_seconds(&self) -> u64 {
        self.timeout_seconds.unwrap_or(300)
    }

    /// Check if verbose logging is enabled (default: false).
    pub fn is_verbose(&self) -> bool {
        self.verbose.unwrap_or(false)
    }

    /// Get a hint for a specific operation or subcommand.
    pub fn get_hint(&self, subcommand: Option<&str>) -> Option<&str> {
        if let Some(hints) = &self.hints {
            // First check for custom hint for the specific subcommand
            if let (Some(cmd), Some(custom_hints)) = (subcommand, &hints.custom)
                && let Some(hint) = custom_hints.get(cmd)
            {
                return Some(hint);
            }

            // Then check for operation-specific hints
            if let Some(subcommand) = subcommand {
                match subcommand {
                    cmd if cmd.contains("build") => return hints.build.as_deref(),
                    cmd if cmd.contains("test") => return hints.test.as_deref(),
                    _ => {}
                }
            }

            // Check for primary hint
            if let Some(primary) = &hints.primary {
                return Some(primary);
            }

            // Finally fallback to default hint
            hints.default.as_deref()
        } else {
            None
        }
    }

    /// Get command override configuration for a specific subcommand.
    pub fn get_command_override(&self, subcommand: &str) -> Option<&CommandOverride> {
        self.overrides.as_ref()?.get(subcommand)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            tool_name: "unknown".to_string(),
            command: None,
            hints: None,
            overrides: None,
            synchronous: Some(false),
            enabled: Some(true),
            timeout_seconds: Some(300),
            verbose: Some(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::tempdir;

    #[test]
    fn test_load_nonexistent_config_fails() {
        let result = Config::load_from_file("nonexistent.toml");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read config file")
        );
    }

    #[test]
    fn test_load_valid_simple_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let toml_content = r#"
tool_name = "cargo"
enabled = true
timeout_seconds = 600
"#;

        write(&config_path, toml_content).unwrap();

        let config = Config::load_from_file(&config_path).unwrap();
        assert_eq!(config.tool_name, "cargo");
        assert!(config.is_enabled());
        assert_eq!(config.get_timeout_seconds(), 600);
        assert_eq!(config.get_command(), "cargo");
    }

    #[test]
    fn test_load_complex_config_with_hints() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let toml_content = r#"
tool_name = "cargo"
command = "cargo"
enabled = true
timeout_seconds = 300
verbose = true

[hints]
build = "Consider reviewing the code while the build runs"
test = "Review test output for patterns and failures"
default = "Use this time to plan next steps"

[hints.custom]
clippy = "Focus on code quality improvements"

[overrides.test]
timeout_seconds = 900
synchronous = false
hint = "Long tests - great time to review docs"
default_args = ["--", "--nocapture"]
"#;

        write(&config_path, toml_content).unwrap();

        let config = Config::load_from_file(&config_path).unwrap();
        assert_eq!(config.tool_name, "cargo");
        assert_eq!(config.get_command(), "cargo");
        assert!(config.is_enabled());
        assert_eq!(config.get_timeout_seconds(), 300);
        assert!(config.is_verbose());

        // Test hints
        assert_eq!(
            config.get_hint(Some("build")),
            Some("Consider reviewing the code while the build runs")
        );
        assert_eq!(
            config.get_hint(Some("test")),
            Some("Review test output for patterns and failures")
        );
        assert_eq!(
            config.get_hint(Some("clippy")),
            Some("Focus on code quality improvements")
        );
        assert_eq!(
            config.get_hint(Some("unknown")),
            Some("Use this time to plan next steps")
        );
        assert_eq!(
            config.get_hint(None),
            Some("Use this time to plan next steps")
        );

        // Test overrides
        let test_override = config.get_command_override("test").unwrap();
        assert_eq!(test_override.timeout_seconds, Some(900));
        assert_eq!(test_override.synchronous, Some(false));
        assert_eq!(
            test_override.hint.as_deref(),
            Some("Long tests - great time to review docs")
        );
        assert_eq!(
            test_override.default_args.as_ref().unwrap(),
            &vec!["--", "--nocapture"]
        );
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.tool_name, "unknown");
        assert!(config.is_enabled());
        assert_eq!(config.get_timeout_seconds(), 300);
        assert!(!config.is_verbose());
        assert_eq!(config.get_command(), "unknown");
        assert!(config.get_hint(Some("build")).is_none());
    }

    #[test]
    fn test_load_tool_config_path() {
        // Create a temporary tools directory structure
        let dir = tempdir().unwrap();
        let tools_dir = dir.path().join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();

        let config_path = tools_dir.join("git.toml");
        let toml_content = r#"
tool_name = "git"
enabled = true
"#;

        write(&config_path, toml_content).unwrap();

        // Change to the temp directory so the tools/ path resolves correctly
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result = Config::load_tool_config("git");

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        let config = result.unwrap();
        assert_eq!(config.tool_name, "git");
        assert!(config.is_enabled());
    }

    #[test]
    fn test_invalid_toml_fails() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("invalid.toml");

        let invalid_toml = r#"
tool_name = "cargo
enabled = definitely_not_a_boolean
"#;

        write(&config_path, invalid_toml).unwrap();

        let result = Config::load_from_file(&config_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse config file")
        );
    }
}
// TODO: Implement configuration validation
