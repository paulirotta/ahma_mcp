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
    let mut final_args: Vec<String> = parts.into_iter().map(String::from).collect();

    if let Some(args_map) = args {
        let positional_arg_names: HashSet<String> = subcommand_config
            .and_then(|sc| sc.positional_args.as_deref())
            .map(|args| args.iter().map(|arg| arg.name.clone()).collect())
            .unwrap_or_default();

        let mut processed_keys = HashSet::new();

        // Check if positional args should come first (e.g., for `find` command)
        let positional_args_first = subcommand_config
            .and_then(|sc| sc.positional_args_first)
            .unwrap_or(false);

        // Process positional args FIRST if configured (e.g., find command where path precedes expressions)
        if positional_args_first
            && let Some(sc) = subcommand_config
            && let Some(pos_args) = &sc.positional_args
        {
            for pos_arg in pos_args {
                if let Some(value) = args_map.get(&pos_arg.name) {
                    process_named_arg(
                        &pos_arg.name,
                        value,
                        &positional_arg_names,
                        subcommand_config,
                        &mut final_args,
                        working_dir,
                        temp_file_manager,
                    )
                    .await?;
                    processed_keys.insert(pos_arg.name.clone());
                }
            }
        }

        // Process options (flags)
        // Process all top-level key-value pairs as named arguments
        // Skip special keys like "args" and meta-parameters that are handled separately
        for (key, value) in args_map {
            // Skip positional args - handled separately based on ordering
            if positional_arg_names.contains(key) {
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
            process_named_arg(
                key,
                value,
                &positional_arg_names,
                subcommand_config,
                &mut final_args,
                working_dir,
                temp_file_manager,
            )
            .await?;
            processed_keys.insert(key.clone());
        }

        // Process positional args AFTER options (default behavior)
        if !positional_args_first
            && let Some(sc) = subcommand_config
            && let Some(pos_args) = &sc.positional_args
        {
            for pos_arg in pos_args {
                if let Some(value) = args_map.get(&pos_arg.name) {
                    process_named_arg(
                        &pos_arg.name,
                        value,
                        &positional_arg_names,
                        subcommand_config,
                        &mut final_args,
                        working_dir,
                        temp_file_manager,
                    )
                    .await?;
                    processed_keys.insert(pos_arg.name.clone());
                }
            }
        }

        // Handle positional arguments from `{"args": [...]}`
        if let Some(inner_args) = args_map.get("args")
            && let Some(positional_values) = inner_args.as_array()
        {
            for value in positional_values {
                if let Some(s) = value_to_string(value).await? {
                    final_args.push(s);
                }
            }
        }
    }

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

/// Helper to process a single named argument.
async fn process_named_arg(
    key: &str,
    value: &Value,
    positional_arg_names: &HashSet<String>,
    subcommand_config: Option<&crate::config::SubcommandConfig>,
    final_args: &mut Vec<String>,
    working_dir: &std::path::Path,
    temp_file_manager: &TempFileManager,
) -> Result<()> {
    // Find if there's a specific config for this argument that indicates file handling
    let file_arg_config = subcommand_config
        .and_then(|sc| sc.options.as_deref())
        .and_then(|opts| {
            opts.iter()
                .find(|opt| opt.name == key && opt.file_arg == Some(true))
        });

    // If configured for file-based argument passing
    if let Some(file_opt) = file_arg_config {
        // Convert the value to a string; this can be None if the JSON value is `null`.
        if let Some(value_str) = value_to_string(value).await? {
            // Only proceed if we have a non-empty string to write.
            if !value_str.is_empty() {
                let temp_file_path = temp_file_manager
                    .create_temp_file_with_content(&value_str)
                    .await?;
                // Use the configured file_flag (e.g., "-F") or a default.
                if let Some(flag) = &file_opt.file_flag {
                    final_args.push(flag.clone());
                } else {
                    // This case should ideally not be hit if config is valid.
                    // The presence of `file_arg: true` implies `file_flag` should exist.
                    final_args.push(format_option_flag(key));
                }
                final_args.push(temp_file_path);
            }
        }
        // If the value is null or empty, we simply don't add any argument.
        return Ok(());
    }

    // Check if this option is defined as boolean type in config
    let is_boolean_option = subcommand_config
        .and_then(|sc| sc.options.as_deref())
        .map(|options| {
            options
                .iter()
                .any(|opt| opt.name == key && opt.option_type == "boolean")
        })
        .unwrap_or(false);

    // Handle boolean values
    if value.as_bool().is_some() || (is_boolean_option && value.as_str().is_some()) {
        let bool_val = if let Some(b) = value.as_bool() {
            b
        } else if let Some(s) = value.as_str() {
            s.eq_ignore_ascii_case("true")
        } else {
            false
        };

        if bool_val {
            let flag = subcommand_config
                .and_then(|sc| sc.options.as_deref())
                .and_then(|options| {
                    options
                        .iter()
                        .find(|opt| opt.name == key)
                        .and_then(|opt| opt.alias.as_ref())
                })
                .map(|alias| format!("-{}", alias))
                .unwrap_or_else(|| format_option_flag(key));
            final_args.push(flag);
        }
        return Ok(());
    }

    // Check if this option is defined as path type in config
    let is_path_option = subcommand_config
        .and_then(|sc| sc.options.as_deref())
        .map(|options| {
            options
                .iter()
                .any(|opt| opt.name == key && opt.format.as_deref() == Some("path"))
        })
        .unwrap_or(false);

    // Also check positional args
    let is_positional_path = subcommand_config
        .and_then(|sc| sc.positional_args.as_deref())
        .map(|args| {
            args.iter()
                .any(|arg| arg.name == key && arg.format.as_deref() == Some("path"))
        })
        .unwrap_or(false);

    // Standard value handling for non-boolean, non-file-arg options.
    // This can return None for `null` values.
    if let Some(value_str) = value_to_string(value).await?
        && !value_str.is_empty()
    {
        let final_value = if is_path_option || is_positional_path {
            let path = std::path::Path::new(&value_str);
            path_security::validate_path(path, working_dir)
                .await?
                .to_string_lossy()
                .to_string()
        } else {
            value_str
        };

        if positional_arg_names.contains(key) {
            final_args.push(final_value);
        } else {
            final_args.push(format_option_flag(key));
            final_args.push(final_value);
        }
    }
    Ok(())
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
