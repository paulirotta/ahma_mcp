//! # CLI Mode
//!
//! Runs the ahma_mcp server in CLI mode, which executes a single tool and prints
//! the result to stdout.

use crate::shell::cli::Cli;
use crate::{
    adapter::Adapter,
    config::{SubcommandConfig, load_tool_configs},
    operation_monitor::{MonitorConfig, OperationMonitor},
    sandbox,
    shell::resolution::{find_matching_tool, resolve_cli_subcommand, run_cli_sequence},
    shell_pool::{ShellPoolConfig, ShellPoolManager},
    tool_availability::evaluate_tool_availability,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

/// Run in CLI mode (execute a single tool and print result).
///
/// # Arguments
/// * `cli` - Command-line arguments.
/// * `sandbox` - Sandbox configuration.
///
/// # Errors
/// Returns an error if the tool execution fails.
pub async fn run_cli_mode(cli: Cli, sandbox: Arc<sandbox::Sandbox>) -> Result<()> {
    let tool_name = cli.tool_name.clone().unwrap();

    // Initialize adapter and monitor for CLI mode
    let monitor_config = MonitorConfig::with_timeout(std::time::Duration::from_secs(cli.timeout));
    let operation_monitor = Arc::new(OperationMonitor::new(monitor_config));
    let shell_pool_config = ShellPoolConfig {
        command_timeout: Duration::from_secs(cli.timeout),
        ..Default::default()
    };
    let shell_pool_manager = Arc::new(ShellPoolManager::new(shell_pool_config));

    // Initialize adapter with sandbox
    let adapter = Adapter::new(
        operation_monitor,
        shell_pool_manager.clone(),
        sandbox.clone(),
    )?;

    // Load tool configurations (now async, no spawn_blocking needed)
    // If tools_dir is None, we'll only have built-in internal tools
    let raw_configs = if let Some(ref tools_dir) = cli.tools_dir {
        load_tool_configs(&cli, tools_dir)
            .await
            .context("Failed to load tool configurations")?
    } else {
        // No tools directory - empty configs (only internal tools)
        HashMap::new()
    };

    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let availability_summary = evaluate_tool_availability(
        shell_pool_manager.clone(),
        raw_configs,
        working_dir.as_path(),
        sandbox.as_ref(), // passed as Arc<Sandbox>
    )
    .await?;

    if !availability_summary.disabled_tools.is_empty() {
        for disabled in &availability_summary.disabled_tools {
            tracing::warn!(
                "Tool '{}' disabled at CLI startup. {}",
                disabled.name,
                disabled.message
            );
        }
    }

    let configs = Arc::new(availability_summary.filtered_configs);

    if configs.is_empty() && tool_name != "sandboxed_shell" {
        tracing::error!("No external tool configurations found");
        anyhow::bail!("No tool '{}' found", tool_name);
    }

    let (config_key, config) = find_matching_tool(configs.as_ref(), &tool_name)?;

    // Check if this is a top-level sequence tool (no subcommands, just sequence)
    let is_top_level_sequence = config.command == "sequence" && config.sequence.is_some();

    let (subcommand_config, command_parts) = if is_top_level_sequence {
        // For top-level sequence tools, create a dummy subcommand config
        // The actual sequence execution happens later
        let dummy_subcommand = SubcommandConfig {
            name: config.name.clone(),
            description: config.description.clone(),
            subcommand: None,
            options: None,
            positional_args: None,
            positional_args_first: None,
            timeout_seconds: config.timeout_seconds,
            synchronous: config.synchronous,
            enabled: true,
            guidance_key: config.guidance_key.clone(),
            sequence: config.sequence.clone(),
            step_delay_ms: config.step_delay_ms,
            availability_check: None,
            install_instructions: None,
        };
        (
            Box::leak(Box::new(dummy_subcommand)) as &SubcommandConfig,
            vec![config.command.clone()],
        )
    } else {
        let (sub, parts) = resolve_cli_subcommand(config_key, config, &tool_name, None)?;
        (sub, parts)
    };

    let mut raw_args = Vec::new();
    let mut working_directory: Option<String> = None;
    let mut tool_args_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Prefer programmatic arguments via environment variable
    if let Ok(env_args) = std::env::var("AHMA_MCP_ARGS") {
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&env_args)
            && let Some(map) = json_val.as_object()
        {
            tool_args_map = map.clone();
        }
    } else {
        let mut iter = cli.tool_args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                raw_args.extend(iter.map(|s| s.to_string()));
                break;
            }

            if arg.starts_with("--") {
                let key = arg.trim_start_matches("--").to_string();
                if let Some(next) = iter.peek() {
                    if next.starts_with('-') {
                        tool_args_map.insert(key, serde_json::Value::Bool(true));
                    } else if let Some(val) = iter.next() {
                        if key == "working-directory" {
                            working_directory = Some(val);
                        } else {
                            tool_args_map.insert(key, serde_json::Value::String(val));
                        }
                    }
                } else {
                    tool_args_map.insert(key, serde_json::Value::Bool(true));
                }
            } else {
                raw_args.push(arg);
            }
        }
    }

    if working_directory.is_none()
        && let Some(wd) = tool_args_map
            .get("working_directory")
            .and_then(|v| v.as_str())
    {
        working_directory = Some(wd.to_string());
    }

    if let Some(args_from_map) = tool_args_map.get("args").and_then(|v| v.as_array()) {
        raw_args.extend(
            args_from_map
                .iter()
                .filter_map(|v| v.as_str().map(String::from)),
        );
    }

    let final_working_dir = working_directory.or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });
    let working_dir_str = final_working_dir.unwrap_or_else(|| ".".to_string());

    let mut args_map = serde_json::Map::new();
    if raw_args.first().map(|s| s.as_str()) == Some("default") {
        raw_args.remove(0);
    }

    for (k, v) in tool_args_map.iter() {
        args_map.insert(k.clone(), v.clone());
    }

    let mut positional_iter = subcommand_config
        .positional_args
        .as_ref()
        .map(|v| v.iter())
        .unwrap_or_else(|| [].iter());

    for arg in &raw_args {
        if let Some((key, value)) = arg.split_once('=') {
            args_map.insert(key.to_string(), Value::String(value.to_string()));
        } else {
            // Try to map to next positional arg
            if let Some(pos_arg) = positional_iter.next() {
                args_map.insert(pos_arg.name.clone(), Value::String(arg.clone()));
            } else {
                // Fallback: treat as key with empty value (old behavior)
                args_map.insert(arg.clone(), Value::String(String::new()));
            }
        }
    }

    if config.command == "sequence" && subcommand_config.sequence.is_some() {
        run_cli_sequence(
            &adapter,
            configs.as_ref(),
            config,
            subcommand_config,
            &working_dir_str,
        )
        .await?;
        return Ok(());
    }

    let base_command = command_parts.join(" ");

    let result = adapter
        .execute_sync_in_dir(
            &base_command,
            Some(args_map),
            &working_dir_str,
            subcommand_config.timeout_seconds,
            Some(subcommand_config),
        )
        .await;

    match result {
        Ok(output) => {
            println!("{}", output);
            Ok(())
        }
        Err(e) => {
            let error_message = e.to_string();
            if error_message.contains("Canceled: Canceled") {
                eprintln!(
                    "Operation cancelled by user request (was: {})",
                    error_message
                );
            } else if error_message.contains("task cancelled for reason") {
                eprintln!(
                    "Operation cancelled by user request or system signal (detected MCP cancellation)"
                );
            } else if error_message.to_lowercase().contains("cancel") {
                eprintln!("Operation cancelled: {}", error_message);
            } else {
                eprintln!("Error executing tool: {}", e);
            }
            Err(anyhow::anyhow!("Tool execution failed"))
        }
    }
}
