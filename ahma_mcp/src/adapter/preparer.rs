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
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use crate::path_security;

enum ArgHandled {
    Handled,
    Skipped,
}

/// Fast lookup/index for option and positional argument metadata.
struct ArgSchemaIndex<'a> {
    options_by_name: HashMap<&'a str, &'a crate::config::CommandOption>,
    positional_by_name: HashMap<&'a str, &'a crate::config::CommandOption>,
    positional_order: Vec<&'a str>,
    positional_names: HashSet<&'a str>,
    positional_args_first: bool,
}

impl<'a> ArgSchemaIndex<'a> {
    fn new(subcommand_config: Option<&'a crate::config::SubcommandConfig>) -> Self {
        let mut options_by_name = HashMap::new();
        let mut positional_by_name = HashMap::new();
        let mut positional_order = Vec::new();
        let mut positional_names = HashSet::new();
        let mut positional_args_first = false;

        if let Some(sc) = subcommand_config {
            positional_args_first = sc.positional_args_first.unwrap_or(false);

            if let Some(options) = sc.options.as_deref() {
                for opt in options {
                    options_by_name.insert(opt.name.as_str(), opt);
                }
            }

            if let Some(positional_args) = sc.positional_args.as_deref() {
                for arg in positional_args {
                    positional_by_name.insert(arg.name.as_str(), arg);
                    positional_order.push(arg.name.as_str());
                    positional_names.insert(arg.name.as_str());
                }
            }
        }

        Self {
            options_by_name,
            positional_by_name,
            positional_order,
            positional_names,
            positional_args_first,
        }
    }

    fn option(&self, name: &str) -> Option<&'a crate::config::CommandOption> {
        self.options_by_name.get(name).copied()
    }

    fn positional(&self, name: &str) -> Option<&'a crate::config::CommandOption> {
        self.positional_by_name.get(name).copied()
    }

    fn is_positional(&self, name: &str) -> bool {
        self.positional_names.contains(name)
    }

    fn positional_args_first(&self) -> bool {
        self.positional_args_first
    }

    fn positional_names_in_order(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.positional_order.iter().copied()
    }

    fn is_path_arg(&self, name: &str) -> bool {
        self.option(name)
            .map(|opt| opt.format.as_deref() == Some("path"))
            .unwrap_or(false)
            || self
                .positional(name)
                .map(|arg| arg.format.as_deref() == Some("path"))
                .unwrap_or(false)
    }
}

/// Encapsulates the state and logic for processing command arguments.
struct ArgProcessor<'a> {
    final_args: Vec<String>,
    working_dir: &'a std::path::Path,
    temp_file_manager: &'a TempFileManager,
    schema: ArgSchemaIndex<'a>,
}

impl<'a> ArgProcessor<'a> {
    fn new(
        initial_args: Vec<String>,
        working_dir: &'a std::path::Path,
        temp_file_manager: &'a TempFileManager,
        subcommand_config: Option<&'a crate::config::SubcommandConfig>,
    ) -> Self {
        Self {
            final_args: initial_args,
            working_dir,
            temp_file_manager,
            schema: ArgSchemaIndex::new(subcommand_config),
        }
    }

    async fn process_named_arg(&mut self, key: &str, value: &Value) -> Result<()> {
        if matches!(
            self.emit_file_arg_if_configured(key, value).await?,
            ArgHandled::Handled
        ) {
            return Ok(());
        }

        if matches!(
            self.emit_boolean_flag_if_true(key, value),
            ArgHandled::Handled
        ) {
            return Ok(());
        }

        self.emit_standard_arg(key, value).await
    }

    async fn emit_file_arg_if_configured(
        &mut self,
        key: &str,
        value: &Value,
    ) -> Result<ArgHandled> {
        let file_arg_config = self
            .schema
            .option(key)
            .filter(|opt| opt.file_arg == Some(true));

        if let Some(file_opt) = file_arg_config {
            if let Some(value_str) = coerce_cli_value(value)?
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
            Ok(ArgHandled::Handled)
        } else {
            Ok(ArgHandled::Skipped)
        }
    }

    fn emit_boolean_flag_if_true(&mut self, key: &str, value: &Value) -> ArgHandled {
        let option_config = self.schema.option(key);
        let is_boolean_option = option_config
            .map(|opt| opt.option_type == "boolean")
            .unwrap_or(false);

        // Check if this looks like a boolean value (native bool or string for boolean options)
        let bool_value = if is_boolean_option {
            resolve_bool(value)
        } else {
            value.as_bool()
        };

        if let Some(bool_val) = bool_value {
            if bool_val {
                let flag = option_config
                    .and_then(|opt| opt.alias.as_ref())
                    .map(|alias| format!("-{}", alias))
                    .unwrap_or_else(|| format_option_flag(key));
                self.final_args.push(flag);
            }
            ArgHandled::Handled
        } else {
            ArgHandled::Skipped
        }
    }

    async fn emit_standard_arg(&mut self, key: &str, value: &Value) -> Result<()> {
        if let Some(value_str) = coerce_cli_value(value)?
            && !value_str.is_empty()
        {
            let final_value = self
                .resolve_validated_path_if_needed(key, value_str)
                .await?;

            if self.schema.is_positional(key) {
                self.final_args.push(final_value);
            } else {
                self.final_args.push(format_option_flag(key));
                self.final_args.push(final_value);
            }
        }
        Ok(())
    }

    async fn resolve_validated_path_if_needed(&self, key: &str, value: String) -> Result<String> {
        if self.schema.is_path_arg(key) {
            let path = std::path::Path::new(&value);
            Ok(path_security::validate_path(path, self.working_dir)
                .await?
                .to_string_lossy()
                .to_string())
        } else {
            Ok(value)
        }
    }

    async fn process_positional_args(&mut self, args_map: &Map<String, Value>) -> Result<()> {
        let positional_names: Vec<&str> = self.schema.positional_names_in_order().collect();
        for positional_name in positional_names {
            if let Some(value) = args_map.get(positional_name) {
                self.process_named_arg(positional_name, value).await?;
            }
        }
        Ok(())
    }

    async fn process_option_args(&mut self, args_map: &Map<String, Value>) -> Result<()> {
        for (key, value) in args_map {
            // Skip positional args - handled separately based on ordering
            if self.schema.is_positional(key) {
                continue;
            }

            // Skip meta-parameters that should not become command-line arguments
            if is_reserved_runtime_key(key) {
                continue;
            }
            self.process_named_arg(key, value).await?;
        }
        Ok(())
    }

    async fn process_explicit_args(&mut self, args_map: &Map<String, Value>) -> Result<()> {
        if let Some(inner_args) = args_map.get("args")
            && let Some(positional_values) = inner_args.as_array()
        {
            for value in positional_values {
                if let Some(s) = coerce_cli_value(value)? {
                    self.final_args.push(s);
                }
            }
        }
        Ok(())
    }

    async fn process_all(&mut self, args: Option<&Map<String, Value>>) -> Result<()> {
        if let Some(args_map) = args {
            let positional_args_first = self.schema.positional_args_first();

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

/// Resolves a boolean value from a JSON value.
/// Handles both native boolean values and string representations ("true"/"false").
fn resolve_bool(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| value.as_str().map(|s| s.eq_ignore_ascii_case("true")))
}

/// Converts a serde_json::Value to a string, handling recursion.
fn coerce_cli_value(value: &Value) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::Bool(b) => Ok(Some(b.to_string())),
        Value::Array(arr) => {
            let mut result = Vec::new();
            for item in arr {
                if let Some(s) = coerce_cli_value(item)? {
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
}

fn is_reserved_runtime_key(key: &str) -> bool {
    matches!(
        key,
        "args" | "working_directory" | "execution_mode" | "timeout_seconds"
    )
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
    use crate::config::{CommandOption, SubcommandConfig};
    use serde_json::json;
    use std::path::Path;

    fn test_temp_manager() -> TempFileManager {
        TempFileManager::new()
    }

    /// Helper to create a CommandOption with minimal boilerplate.
    fn make_option(name: &str, option_type: &str) -> CommandOption {
        CommandOption {
            name: name.to_string(),
            option_type: option_type.to_string(),
            description: None,
            required: None,
            format: None,
            items: None,
            file_arg: None,
            file_flag: None,
            alias: None,
        }
    }

    /// Helper to create a CommandOption with path format.
    fn make_path_option(name: &str, option_type: &str) -> CommandOption {
        CommandOption {
            name: name.to_string(),
            option_type: option_type.to_string(),
            format: Some("path".to_string()),
            description: None,
            required: None,
            items: None,
            file_arg: None,
            file_flag: None,
            alias: None,
        }
    }

    /// Helper to create a SubcommandConfig for the find command.
    fn make_find_subcommand() -> SubcommandConfig {
        SubcommandConfig {
            name: "find".to_string(),
            description: "Search for files".to_string(),
            enabled: true,
            positional_args_first: Some(true),
            positional_args: Some(vec![make_path_option("path", "string")]),
            options: Some(vec![
                make_option("-name", "string"),
                make_option("-maxdepth", "integer"),
            ]),
            subcommand: None,
            timeout_seconds: None,
            synchronous: None,
            guidance_key: None,
            sequence: None,
            step_delay_ms: None,
            availability_check: None,
            install_instructions: None,
        }
    }

    fn make_bool_option(name: &str, alias: &str) -> CommandOption {
        CommandOption {
            name: name.to_string(),
            option_type: "boolean".to_string(),
            description: None,
            required: None,
            format: None,
            items: None,
            file_arg: None,
            file_flag: None,
            alias: Some(alias.to_string()),
        }
    }

    fn make_file_option(name: &str, flag: Option<&str>) -> CommandOption {
        CommandOption {
            name: name.to_string(),
            option_type: "string".to_string(),
            description: None,
            required: None,
            format: None,
            items: None,
            file_arg: Some(true),
            file_flag: flag.map(str::to_string),
            alias: None,
        }
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
        let temp_manager = test_temp_manager();
        let subcommand_config = make_find_subcommand();

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

    #[tokio::test]
    async fn boolean_option_uses_alias_when_true() {
        let temp_manager = test_temp_manager();
        let subcommand_config = SubcommandConfig {
            name: "demo".to_string(),
            description: "demo".to_string(),
            enabled: true,
            positional_args_first: None,
            positional_args: None,
            options: Some(vec![make_bool_option("verbose", "v")]),
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
        args_map.insert("verbose".to_string(), json!("true"));

        let (_, args_vec) = prepare_command_and_args(
            "mycmd",
            Some(&args_map),
            Some(&subcommand_config),
            Path::new("."),
            &temp_manager,
        )
        .await
        .expect("command");

        assert_eq!(args_vec, vec!["-v".to_string()]);
    }

    #[tokio::test]
    async fn reserved_runtime_keys_are_not_emitted_as_cli_args() {
        let temp_manager = test_temp_manager();
        let mut args_map = Map::new();
        args_map.insert("working_directory".to_string(), json!("/tmp"));
        args_map.insert("execution_mode".to_string(), json!("Synchronous"));
        args_map.insert("timeout_seconds".to_string(), json!(5));
        args_map.insert("args".to_string(), json!(["positional"]));
        args_map.insert("name".to_string(), json!("value"));

        let (_, args_vec) = prepare_command_and_args(
            "mycmd",
            Some(&args_map),
            None,
            Path::new("."),
            &temp_manager,
        )
        .await
        .expect("command");

        assert_eq!(
            args_vec,
            vec![
                "--name".to_string(),
                "value".to_string(),
                "positional".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn file_arg_uses_configured_flag_and_writes_content() {
        let temp_manager = test_temp_manager();
        let subcommand_config = SubcommandConfig {
            name: "demo".to_string(),
            description: "demo".to_string(),
            enabled: true,
            positional_args_first: None,
            positional_args: None,
            options: Some(vec![make_file_option("input", Some("-f"))]),
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
        args_map.insert("input".to_string(), json!("line 1\nline 2"));

        let (_, args_vec) = prepare_command_and_args(
            "mycmd",
            Some(&args_map),
            Some(&subcommand_config),
            Path::new("."),
            &temp_manager,
        )
        .await
        .expect("command");

        assert_eq!(args_vec.len(), 2);
        assert_eq!(args_vec[0], "-f");

        let path = std::path::PathBuf::from(&args_vec[1]);
        assert!(
            path.exists(),
            "Expected temp file to exist: {}",
            args_vec[1]
        );
        let contents =
            std::fs::read_to_string(&path).expect("failed to read generated temp file content");
        assert_eq!(contents, "line 1\nline 2");
    }
}
