//! Command preparation logic for the adapter.
//!
//! This module handles the conversion of structured command arguments into
//! executable command-line arguments, including:
//! - Parsing command strings
//! - Converting JSON arguments to CLI flags and positional arguments
//! - Handling file-based argument passing for complex values
//! - Path validation and security checks

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use crate::path_security;

/// Encapsulates the state and logic for processing command arguments.
struct ArgProcessor<'a> {
    final_args: Vec<String>,
    processed_keys: HashSet<String>,
    working_dir: &'a std::path::Path,
    temp_file_manager: &'a TempFileManager,
    subcommand_config: Option<&'a crate::config::SubcommandConfig>,
    positional_arg_names: HashSet<String>,
}

impl<'a> ArgProcessor<'a> {
    fn new(
        initial_args: Vec<String>,
        working_dir: &'a std::path::Path,
        temp_file_manager: &'a TempFileManager,
        subcommand_config: Option<&'a crate::config::SubcommandConfig>,
    ) -> Self {
        let positional_arg_names: HashSet<String> = subcommand_config
            .and_then(|sc| sc.positional_args.as_deref())
            .map(|args| args.iter().map(|arg| arg.name.clone()).collect())
            .unwrap_or_default();

        Self {
            final_args: initial_args,
            processed_keys: HashSet::new(),
            working_dir,
            temp_file_manager,
            subcommand_config,
            positional_arg_names,
        }
    }

    /// Helper to process a single key-value argument.
    async fn process_arg_kv(&mut self, key: &str, value: &Value) -> Result<()> {
        if self.handle_file_arg(key, value).await? {
            return Ok(());
        }

        if self.handle_boolean_arg(key, value).await? {
            return Ok(());
        }

        self.handle_standard_arg(key, value).await
    }

    async fn handle_file_arg(&mut self, key: &str, value: &Value) -> Result<bool> {
        let file_arg_config = self
            .subcommand_config
            .and_then(|sc| sc.options.as_deref())
            .and_then(|opts| {
                opts.iter()
                    .find(|opt| opt.name == key && opt.file_arg == Some(true))
            });

        if let Some(file_opt) = file_arg_config {
            if let Some(value_str) = value_to_string(value).await?
                && !value_str.is_empty()
            {
                let temp_file_path = self
                    .temp_file_manager
                    .create_temp_file_with_content(&value_str)
                    .await?;
                if let Some(flag) = &file_opt.file_flag {
                    self.final_args.push(flag.clone());
                } else {
                    self.final_args.push(format_option_flag(key));
                }
                self.final_args.push(temp_file_path);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn handle_boolean_arg(&mut self, key: &str, value: &Value) -> Result<bool> {
        let is_boolean_option = self
            .subcommand_config
            .and_then(|sc| sc.options.as_deref())
            .map(|options| {
                options
                    .iter()
                    .any(|opt| opt.name == key && opt.option_type == "boolean")
            })
            .unwrap_or(false);

        if value.as_bool().is_some() || (is_boolean_option && value.as_str().is_some()) {
            let bool_val = if let Some(b) = value.as_bool() {
                b
            } else if let Some(s) = value.as_str() {
                s.eq_ignore_ascii_case("true")
            } else {
                false
            };

            if bool_val {
                let flag = self
                    .subcommand_config
                    .and_then(|sc| sc.options.as_deref())
                    .and_then(|options| {
                        options
                            .iter()
                            .find(|opt| opt.name == key)
                            .and_then(|opt| opt.alias.as_ref())
                    })
                    .map(|alias| format!("-{}", alias))
                    .unwrap_or_else(|| format_option_flag(key));
                self.final_args.push(flag);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn handle_standard_arg(&mut self, key: &str, value: &Value) -> Result<()> {
        let is_path_option = self
            .subcommand_config
            .and_then(|sc| sc.options.as_deref())
            .map(|options| {
                options
                    .iter()
                    .any(|opt| opt.name == key && opt.format.as_deref() == Some("path"))
            })
            .unwrap_or(false);

        let is_positional_path = self
            .subcommand_config
            .and_then(|sc| sc.positional_args.as_deref())
            .map(|args| {
                args.iter()
                    .any(|arg| arg.name == key && arg.format.as_deref() == Some("path"))
            })
            .unwrap_or(false);

        if let Some(value_str) = value_to_string(value).await?
            && !value_str.is_empty()
        {
            let final_value = if is_path_option || is_positional_path {
                let path = std::path::Path::new(&value_str);
                path_security::validate_path(path, self.working_dir)
                    .await?
                    .to_string_lossy()
                    .to_string()
            } else {
                value_str
            };

            if self.positional_arg_names.contains(key) {
                self.final_args.push(final_value);
            } else {
                self.final_args.push(format_option_flag(key));
                self.final_args.push(final_value);
            }
        }
        Ok(())
    }

    async fn process_positional_args(&mut self, args_map: &Map<String, Value>) -> Result<()> {
        if let Some(sc) = self.subcommand_config
            && let Some(pos_args) = &sc.positional_args
        {
            for pos_arg in pos_args {
                if let Some(value) = args_map.get(&pos_arg.name) {
                    self.process_arg_kv(&pos_arg.name, value).await?;
                    self.processed_keys.insert(pos_arg.name.clone());
                }
            }
        }
        Ok(())
    }

    async fn process_option_args(&mut self, args_map: &Map<String, Value>) -> Result<()> {
        for (key, value) in args_map {
            // Skip positional args - handled separately based on ordering
            if self.positional_arg_names.contains(key) {
                continue;
            }

            // Skip meta-parameters that should not become command-line arguments
            if key == "args"
                || key == "working_directory"
                || key == "execution_mode"
                || key == "timeout_seconds"
            {
                continue;
            }
            self.process_arg_kv(key, value).await?;
            self.processed_keys.insert(key.clone());
        }
        Ok(())
    }

    async fn process_explicit_args(&mut self, args_map: &Map<String, Value>) -> Result<()> {
        if let Some(inner_args) = args_map.get("args")
            && let Some(positional_values) = inner_args.as_array()
        {
            for value in positional_values {
                if let Some(s) = value_to_string(value).await? {
                    self.final_args.push(s);
                }
            }
        }
        Ok(())
    }

    async fn process_all(&mut self, args: Option<&Map<String, Value>>) -> Result<()> {
        if let Some(args_map) = args {
            let positional_args_first = self
                .subcommand_config
                .and_then(|sc| sc.positional_args_first)
                .unwrap_or(false);

            if positional_args_first {
                self.process_positional_args(args_map).await?;
            }

            self.process_option_args(args_map).await?;

            if !positional_args_first {
                self.process_positional_args(args_map).await?;
            }

            self.process_explicit_args(args_map).await?;
        }
        Ok(())
    }
}

/// Manages temporary files created for complex command arguments.
#[derive(Debug, Clone)]
pub struct TempFileManager {
    temp_files: Arc<Mutex<Vec<NamedTempFile>>>,
}

impl TempFileManager {
    /// Creates a new temporary file manager.
    pub fn new() -> Self {
        Self {
            temp_files: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a temporary file with the given content and returns the file path.
    pub async fn create_temp_file_with_content(&self, content: &str) -> Result<String> {
        let mut temp_file = NamedTempFile::new()
            .context("Failed to create temporary file for multi-line argument")?;

        // Perform the blocking write on Tokio's blocking thread pool.
        // Note: spawn_blocking is appropriate here per R16.3 - the tempfile crate
        // only offers synchronous write APIs.
        let temp_file = {
            // Move the NamedTempFile into the blocking task and return it after write.
            let content = content.to_owned();
            tokio::task::spawn_blocking(move || -> Result<NamedTempFile> {
                temp_file
                    .write_all(content.as_bytes())
                    .context("Failed to write content to temporary file")?;
                temp_file
                    .flush()
                    .context("Failed to flush temporary file")?;
                Ok(temp_file)
            })
            .await
            .context("Failed to run blocking write in background")??
        };

        let file_path = temp_file.path().to_string_lossy().to_string();

        // Store the temp file so it doesn't get cleaned up until the manager is dropped
        {
            let mut temp_files = self.temp_files.lock().await;
            temp_files.push(temp_file);
        }

        Ok(file_path)
    }
}

impl Default for TempFileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses the command string and arguments into a program and argument list.
///
/// This helper handles complications such as:
/// - Splitting the base command string (e.g., "python script.py" -> program: "python", args: ["script.py"])
/// - Converting structured JSON arguments into CLI flags and positional arguments
/// - Applying subcommand configurations (aliases, hardcoded args)
/// - Creating temporary files for multi-line string arguments
pub async fn prepare_command_and_args(
    command: &str,
    args: Option<&Map<String, Value>>,
    subcommand_config: Option<&crate::config::SubcommandConfig>,
    working_dir: &std::path::Path,
    temp_file_manager: &TempFileManager,
) -> Result<(String, Vec<String>)> {
    let mut parts: Vec<&str> = command.split_whitespace().collect();

    if parts.is_empty() {
        anyhow::bail!("Command must not be empty");
    }

    let program = parts.remove(0).to_string();

    // The remaining parts from the command string become the initial args.
    // Note: The subcommand name is already included in the command string
    // (added by mcp_service.rs), so we do NOT add it again here.
    let initial_args: Vec<String> = parts.into_iter().map(String::from).collect();

    let mut processor = ArgProcessor::new(
        initial_args,
        working_dir,
        temp_file_manager,
        subcommand_config,
    );

    processor.process_all(args).await?;
    let mut final_args = processor.final_args;

    maybe_append_shell_redirect(&program, &mut final_args);

    Ok((program, final_args))
}

fn maybe_append_shell_redirect(program: &str, args: &mut Vec<String>) {
    if let Some(idx) = shell_script_index(program, args.as_slice())
        && let Some(script) = args.get_mut(idx)
    {
        ensure_shell_redirect(script);
    }
}

fn shell_script_index(program: &str, args: &[String]) -> Option<usize> {
    if !is_shell_program(program) {
        return None;
    }
    let command_idx = args.iter().position(|a| a == "-c")?;
    let script_idx = command_idx + 1;
    if script_idx < args.len() {
        Some(script_idx)
    } else {
        None
    }
}

fn ensure_shell_redirect(script: &mut String) {
    if script.trim_end().ends_with("2>&1") {
        return;
    }

    let needs_space = script
        .chars()
        .last()
        .map(|c| !c.is_whitespace())
        .unwrap_or(false);

    if needs_space {
        script.push(' ');
    }
    script.push_str("2>&1");
}

fn is_shell_program(program: &str) -> bool {
    matches!(
        program,
        "sh" | "bash" | "zsh" | "/bin/sh" | "/bin/bash" | "/bin/zsh"
    )
}

/// Converts a serde_json::Value to a string, handling recursion with boxing.
fn value_to_string<'a>(
    value: &'a Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>> {
    Box::pin(async move {
        match value {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s.clone())),
            Value::Number(n) => Ok(Some(n.to_string())),
            Value::Bool(b) => Ok(Some(b.to_string())),
            Value::Array(arr) => {
                let mut result = Vec::new();
                for item in arr {
                    if let Some(s) = value_to_string(item).await? {
                        result.push(s);
                    }
                }
                if result.is_empty() {
                    return Ok(None);
                }
                Ok(Some(result.join(" ")))
            }
            // For other types like Object, we don't want to convert them to a string.
            _ => Ok(None),
        }
    })
}

/// Checks if a string contains characters that are problematic for shell argument passing
pub fn needs_file_handling(value: &str) -> bool {
    value.contains('\n')
        || value.contains('\r')
        || value.contains('\'')
        || value.contains('"')
        || value.contains('\\')
        || value.contains('`')
        || value.contains('$')
        || value.len() > 8192 // Also handle very long arguments via file
}

/// Formats an option name as a command-line flag.
///
/// If the option name already starts with a dash (e.g., "-name" for `find`),
/// it's used as-is. Otherwise, it's prefixed with "--" for standard long options.
pub fn format_option_flag(key: &str) -> String {
    if key.starts_with('-') {
        key.to_string()
    } else {
        format!("--{}", key)
    }
}

/// Prepare a string for shell argument passing by escaping special characters.
///
/// This function wraps the string in single quotes and handles any embedded single quotes.
///
/// # Purpose
///
/// "Escaping" here means neutralizing special characters (like spaces, `$`, quotes, etc.)
/// so the shell treats the value as a single piece of text (a literal string) rather than
/// interpreting it as code or multiple arguments. This prevents "shell injection" attacks.
///
/// # When to use
///
/// Use this only as a fallback when you cannot use file-based data passing. Passing data
/// through temporary files is generally robust, but if you must construct a raw command
/// string with arguments, this function ensures those arguments are safe.
pub fn escape_shell_argument(value: &str) -> String {
    // Use single quotes and escape any embedded single quotes
    if value.contains('\'') {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    } else {
        format!("'{}'", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    fn test_temp_manager() -> TempFileManager {
        TempFileManager::new()
    }

    #[tokio::test]
    async fn shell_commands_append_redirect_once() {
        let temp_manager = test_temp_manager();
        let mut args_map = Map::new();
        args_map.insert("args".to_string(), json!(["echo hi"]));

        let (program, args_vec) = prepare_command_and_args(
            "/bin/sh -c",
            Some(&args_map),
            None,
            Path::new("."),
            &temp_manager,
        )
        .await
        .expect("command");

        assert_eq!(program, "/bin/sh");
        assert_eq!(args_vec, vec!["-c".to_string(), "echo hi 2>&1".to_string()]);
    }

    #[tokio::test]
    async fn shell_commands_do_not_duplicate_redirect() {
        let temp_manager = test_temp_manager();
        let mut args_map = Map::new();
        args_map.insert("args".to_string(), json!(["ls 2>&1"]));

        let (_, args_vec) = prepare_command_and_args(
            "/bin/sh -c",
            Some(&args_map),
            None,
            Path::new("."),
            &temp_manager,
        )
        .await
        .expect("command");

        assert_eq!(args_vec, vec!["-c".to_string(), "ls 2>&1".to_string()]);
    }

    #[tokio::test]
    async fn non_shell_commands_remain_unchanged() {
        let temp_manager = test_temp_manager();
        let mut args_map = Map::new();
        args_map.insert("args".to_string(), json!(["--version"]));

        let (program, args_vec) =
            prepare_command_and_args("git", Some(&args_map), None, Path::new("."), &temp_manager)
                .await
                .expect("command");

        assert_eq!(program, "git");
        assert_eq!(args_vec, vec!["--version".to_string()]);
    }

    #[test]
    fn test_format_option_flag_standard_option() {
        // Standard options get -- prefix
        assert_eq!(format_option_flag("verbose"), "--verbose");
        assert_eq!(format_option_flag("force"), "--force");
        assert_eq!(
            format_option_flag("working_directory"),
            "--working_directory"
        );
    }

    #[test]
    fn test_format_option_flag_dash_prefixed_option() {
        // Options already starting with - are used as-is
        assert_eq!(format_option_flag("-name"), "-name");
        assert_eq!(format_option_flag("-type"), "-type");
        assert_eq!(format_option_flag("-mtime"), "-mtime");
        // Double-dash options are also preserved
        assert_eq!(format_option_flag("--version"), "--version");
    }

    #[test]
    fn test_format_option_flag_empty_string() {
        // Empty string should get -- prefix (edge case)
        assert_eq!(format_option_flag(""), "--");
    }

    #[tokio::test]
    async fn find_command_args_with_dash_prefix() {
        use crate::config::{CommandOption, SubcommandConfig};

        let temp_manager = test_temp_manager();

        // Create a subcommand config that matches the find subcommand in file_tools.json
        let subcommand_config = SubcommandConfig {
            name: "find".to_string(),
            description: "Search for files".to_string(),
            enabled: true,
            positional_args_first: Some(true), // find requires path before options
            positional_args: Some(vec![CommandOption {
                name: "path".to_string(),
                description: None,
                required: Some(false),
                option_type: "string".to_string(),
                format: Some("path".to_string()),
                items: None,
                file_arg: None,
                file_flag: None,
                alias: None,
            }]),
            options: Some(vec![
                CommandOption {
                    name: "-name".to_string(),
                    option_type: "string".to_string(),
                    description: Some("Search pattern".to_string()),
                    required: None,
                    format: None,
                    items: None,
                    file_arg: None,
                    file_flag: None,
                    alias: None,
                },
                CommandOption {
                    name: "-maxdepth".to_string(),
                    option_type: "integer".to_string(),
                    description: Some("Max depth".to_string()),
                    required: None,
                    format: None,
                    items: None,
                    file_arg: None,
                    file_flag: None,
                    alias: None,
                },
            ]),
            subcommand: None,
            timeout_seconds: None,
            synchronous: None,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        };

        let mut args_map = Map::new();
        args_map.insert("path".to_string(), json!("."));
        args_map.insert("-name".to_string(), json!("*.toml"));
        args_map.insert("-maxdepth".to_string(), json!(1));

        let (program, args_vec) = prepare_command_and_args(
            "find",
            Some(&args_map),
            Some(&subcommand_config),
            Path::new("."),
            &temp_manager,
        )
        .await
        .expect("command");

        assert_eq!(program, "find");
        // With positional_args_first: true, path should come BEFORE options
        // This is required by both BSD and GNU find
        assert!(
            args_vec.contains(&"-name".to_string()),
            "Should contain -name, got: {:?}",
            args_vec
        );
        assert!(
            args_vec.contains(&"-maxdepth".to_string()),
            "Should contain -maxdepth, got: {:?}",
            args_vec
        );
        assert!(
            args_vec.contains(&"*.toml".to_string()),
            "Should contain pattern value, got: {:?}",
            args_vec
        );
        assert!(
            args_vec.contains(&"1".to_string()),
            "Should contain depth value, got: {:?}",
            args_vec
        );
        // With positional_args_first: true, the path should be the first argument
        // (path is expanded to absolute path due to format: "path")
        let first_arg = args_vec.first().expect("Should have at least one argument");
        assert!(
            first_arg.starts_with('/') || first_arg == ".",
            "First argument should be a path, got: {:?}",
            args_vec
        );
        // Verify path comes before options
        let name_idx = args_vec.iter().position(|s| s == "-name").unwrap();
        let maxdepth_idx = args_vec.iter().position(|s| s == "-maxdepth").unwrap();
        assert!(
            0 < name_idx && 0 < maxdepth_idx,
            "Path (index 0) should come before options (-name at {}, -maxdepth at {}): {:?}",
            name_idx,
            maxdepth_idx,
            args_vec
        );
        // Should NOT contain --maxdepth or ---name
        assert!(
            !args_vec.iter().any(|s| s == "--maxdepth"),
            "Should NOT contain --maxdepth, got: {:?}",
            args_vec
        );
        assert!(
            !args_vec.iter().any(|s| s == "---name"),
            "Should NOT contain ---name, got: {:?}",
            args_vec
        );
    }
}
