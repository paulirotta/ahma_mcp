//! Tool availability probing and install guidance.
//!
//! This module runs lightweight probes to decide whether tools and subcommands
//! are available in the current environment. It returns a summary containing
//! filtered tool configs and human-friendly guidance for missing dependencies.
//!
//! ## Security
//! Availability probes are executed inside the configured sandbox scope to avoid
//! untrusted filesystem access.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use crate::config::ToolConfig;
use crate::shell_pool::ShellPoolManager;
use anyhow::Result;

/// Summary of a disabled tool and why it was disabled.
#[derive(Debug, Clone)]
pub struct DisabledTool {
    /// Tool name (matching the config key).
    pub name: String,
    /// Human-readable message describing the failure.
    pub message: String,
    /// Optional install guidance for the tool.
    pub install_instructions: Option<String>,
}

/// Summary of a disabled subcommand and why it was disabled.
#[derive(Debug, Clone)]
pub struct DisabledSubcommand {
    /// Parent tool name.
    pub tool: String,
    /// Subcommand path (e.g., "git commit").
    pub subcommand_path: String,
    /// Human-readable message describing the failure.
    pub message: String,
    /// Optional install guidance for the subcommand.
    pub install_instructions: Option<String>,
}

/// Aggregate results of running availability probes.
#[derive(Debug, Clone)]
pub struct AvailabilitySummary {
    /// Tool configs that remain enabled after probing.
    pub filtered_configs: HashMap<String, ToolConfig>,
    /// Tools that were disabled due to failed probes.
    pub disabled_tools: Vec<DisabledTool>,
    /// Subcommands that were disabled due to failed probes.
    pub disabled_subcommands: Vec<DisabledSubcommand>,
}

// ---------------------------------------------------------------------------
// Small utility helpers
// ---------------------------------------------------------------------------

/// Append a single disabled-item entry to the guidance output.
fn write_disabled_item(
    output: &mut String,
    name: &str,
    message: &str,
    install_instructions: Option<&str>,
) {
    let _ = writeln!(output, "- {}: {}", name, message);
    if let Some(trimmed) = install_instructions
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let _ = writeln!(output, "  install: {}", trimmed);
    }
}

/// Build a human-friendly message summarizing install guidance for disabled items.
pub fn format_install_guidance(summary: &AvailabilitySummary) -> String {
    if summary.disabled_tools.is_empty() && summary.disabled_subcommands.is_empty() {
        return "All configured tools are available.".to_string();
    }

    let mut output = String::new();

    if !summary.disabled_tools.is_empty() {
        output.push_str("Disabled tools requiring attention:\n");
        for tool in &summary.disabled_tools {
            write_disabled_item(
                &mut output,
                &tool.name,
                &tool.message,
                tool.install_instructions.as_deref(),
            );
        }
    }

    if !summary.disabled_subcommands.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("Disabled subcommands requiring attention:\n");
        for sub in &summary.disabled_subcommands {
            let name = format!("{}::{}", sub.tool, sub.subcommand_path);
            write_disabled_item(
                &mut output,
                &name,
                &sub.message,
                sub.install_instructions.as_deref(),
            );
        }
    }

    output.trim_end().to_string()
}

mod builder;
mod engine;
mod types;

/// Evaluate tool availability in parallel using the shell pool. Tools or subcommands whose
/// probes fail will be disabled and recorded in the returned summary.
pub async fn evaluate_tool_availability(
    shell_pool: Arc<ShellPoolManager>,
    configs: HashMap<String, ToolConfig>,
    default_working_dir: &Path,
    sandbox: &crate::sandbox::Sandbox,
) -> Result<AvailabilitySummary> {
    engine::evaluate_tool_availability_impl(shell_pool, configs, default_working_dir, sandbox).await
}
