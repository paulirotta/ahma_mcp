use anyhow::Result;
use serde_json::{Map, Value};
use tracing::warn;

use crate::path_security;

use super::arg_schema::ArgSchemaIndex;
use super::conversions::{
    coerce_cli_value, format_option_flag, is_reserved_runtime_key, resolve_bool,
};
use super::temp_file::TempFileManager;

enum ArgHandled {
    Handled,
    Skipped,
}

/// Encapsulates the state and logic for processing command arguments.
pub(super) struct ArgProcessor<'a> {
    final_args: Vec<String>,
    working_dir: &'a std::path::Path,
    temp_file_manager: &'a TempFileManager,
    schema: ArgSchemaIndex<'a>,
}

impl<'a> ArgProcessor<'a> {
    pub(super) fn new(
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

    pub(super) fn into_final_args(self) -> Vec<String> {
        self.final_args
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

            // When a schema is present, reject unknown argument keys rather than
            // blindly emitting them as --{key} flags (which causes errors like --source
            // being passed to grep).
            if self.schema.has_schema() && !self.schema.is_known_arg(key) {
                warn!(
                    "Ignoring unknown argument '{}' — not defined in tool schema. \
                     This prevents invalid flags from being passed to the command.",
                    key
                );
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

    pub(super) async fn process_all(&mut self, args: Option<&Map<String, Value>>) -> Result<()> {
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
