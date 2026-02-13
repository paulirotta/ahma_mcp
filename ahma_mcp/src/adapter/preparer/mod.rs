//! Command preparation logic for the adapter.
//!
//! This module handles the conversion of structured command arguments into
//! executable command-line arguments, including:
//! - Parsing command strings
//! - Converting JSON arguments to CLI flags and positional arguments
//! - Handling file-based argument passing for complex values
//! - Path validation and security checks

mod arg_processor;
mod arg_schema;
mod conversions;
mod shell_redirect;
mod temp_file;

#[cfg(test)]
mod tests;

use anyhow::Result;
use serde_json::{Map, Value};

use self::arg_processor::ArgProcessor;
pub use self::conversions::{escape_shell_argument, format_option_flag, needs_file_handling};
use self::shell_redirect::maybe_append_shell_redirect;
pub use self::temp_file::TempFileManager;

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
    let mut final_args = processor.into_final_args();

    maybe_append_shell_redirect(&program, &mut final_args);

    Ok((program, final_args))
}
