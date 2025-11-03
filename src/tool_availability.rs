use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::{AvailabilityCheck, SubcommandConfig, ToolConfig};
use crate::shell_pool::{ShellCommand, ShellPoolManager};

const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone)]
pub struct DisabledTool {
    pub name: String,
    pub message: String,
    pub install_instructions: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DisabledSubcommand {
    pub tool: String,
    pub subcommand_path: String,
    pub message: String,
    pub install_instructions: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AvailabilitySummary {
    pub filtered_configs: HashMap<String, ToolConfig>,
    pub disabled_tools: Vec<DisabledTool>,
    pub disabled_subcommands: Vec<DisabledSubcommand>,
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
            let _ = writeln!(output, "- {}: {}", tool.name, tool.message);
            if let Some(hint) = tool.install_instructions.as_ref() {
                let trimmed = hint.trim();
                if !trimmed.is_empty() {
                    let _ = writeln!(output, "  install: {}", trimmed);
                }
            }
        }
    }

    if !summary.disabled_subcommands.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("Disabled subcommands requiring attention:\n");
        for sub in &summary.disabled_subcommands {
            let _ = writeln!(
                output,
                "- {}::{}: {}",
                sub.tool, sub.subcommand_path, sub.message
            );
            if let Some(hint) = sub.install_instructions.as_ref() {
                let trimmed = hint.trim();
                if !trimmed.is_empty() {
                    let _ = writeln!(output, "  install: {}", trimmed);
                }
            }
        }
    }

    output.trim_end().to_string()
}

#[derive(Debug, Clone)]
enum ProbeTarget {
    Tool { name: String },
    Subcommand { tool: String, path: Vec<String> },
}

#[derive(Debug, Clone)]
struct ProbePlan {
    target: ProbeTarget,
    command: Vec<String>,
    working_dir: PathBuf,
    success_codes: Vec<i32>,
    timeout_ms: u64,
    install_instructions: Option<String>,
}

#[derive(Debug)]
struct ProbeOutcome {
    plan: ProbePlan,
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Evaluate tool availability in parallel using the shell pool. Tools or subcommands whose
/// probes fail will be disabled and recorded in the returned summary.
pub async fn evaluate_tool_availability(
    shell_pool: Arc<ShellPoolManager>,
    configs: HashMap<String, ToolConfig>,
    default_working_dir: &Path,
) -> Result<AvailabilitySummary> {
    if configs.is_empty() {
        return Ok(AvailabilitySummary {
            filtered_configs: configs,
            disabled_tools: Vec::new(),
            disabled_subcommands: Vec::new(),
        });
    }

    let mut filtered_configs = configs;

    let plans = build_probe_plans(&filtered_configs, default_working_dir);
    debug!("Prepared {} availability probe plans", plans.len());

    if plans.is_empty() {
        return Ok(AvailabilitySummary {
            filtered_configs,
            disabled_tools: Vec::new(),
            disabled_subcommands: Vec::new(),
        });
    }

    // Run probes concurrently without blocking the current runtime
    let probe_tasks: Vec<_> = plans
        .into_iter()
        .map(|plan| {
            let shell_pool = shell_pool.clone();
            tokio::spawn(async move { run_probe(shell_pool, plan).await })
        })
        .collect();

    let outcomes: Vec<ProbeOutcome> = futures::future::join_all(probe_tasks)
        .await
        .into_iter()
        .filter_map(|result| result.ok())
        .collect();

    let mut disabled_tools = Vec::new();
    let mut disabled_subcommands = Vec::new();

    for outcome in outcomes {
        let stdout = outcome.stdout.trim();
        let stderr = outcome.stderr.trim();

        if outcome.success {
            debug!(
                "Probe success for {:?} (exit {:?}) stdout: {} stderr: {}",
                outcome.plan.target,
                outcome.exit_code,
                if stdout.is_empty() { "<empty>" } else { stdout },
                if stderr.is_empty() { "<empty>" } else { stderr },
            );
            continue;
        }

        match outcome.plan.target {
            ProbeTarget::Tool { ref name } => {
                if let Some(config) = filtered_configs.get_mut(name) {
                    config.enabled = false;
                }
                let message = format!(
                    "Tool '{}' disabled. Probe command {:?} failed with exit {:?}. stdout: {} stderr: {}",
                    name,
                    outcome.plan.command,
                    outcome.exit_code,
                    if stdout.is_empty() { "<empty>" } else { stdout },
                    if stderr.is_empty() { "<empty>" } else { stderr },
                );
                warn!("{message}");
                if let Some(instructions) = &outcome.plan.install_instructions {
                    info!("Install hint for '{}': {}", name, instructions.trim());
                }
                disabled_tools.push(DisabledTool {
                    name: name.clone(),
                    message,
                    install_instructions: outcome.plan.install_instructions.clone(),
                });
            }
            ProbeTarget::Subcommand { ref tool, ref path } => {
                if let Some(config) = filtered_configs.get_mut(tool) {
                    if let Some(sub) = find_subcommand_mut(&mut config.subcommand, path) {
                        sub.enabled = false;
                    }
                }

                let joined_path = path.join("_");
                let message = format!(
                    "Subcommand '{}::{}' disabled. Probe command {:?} failed with exit {:?}. stdout: {} stderr: {}",
                    tool,
                    joined_path,
                    outcome.plan.command,
                    outcome.exit_code,
                    if stdout.is_empty() { "<empty>" } else { stdout },
                    if stderr.is_empty() { "<empty>" } else { stderr },
                );
                warn!("{message}");
                if let Some(instructions) = &outcome.plan.install_instructions {
                    info!(
                        "Install hint for '{}::{}': {}",
                        tool,
                        joined_path,
                        instructions.trim()
                    );
                }
                disabled_subcommands.push(DisabledSubcommand {
                    tool: tool.clone(),
                    subcommand_path: joined_path,
                    message,
                    install_instructions: outcome.plan.install_instructions.clone(),
                });
            }
        }
    }

    Ok(AvailabilitySummary {
        filtered_configs,
        disabled_tools,
        disabled_subcommands,
    })
}

fn build_probe_plans(
    configs: &HashMap<String, ToolConfig>,
    default_working_dir: &Path,
) -> Vec<ProbePlan> {
    let mut plans = Vec::new();

    for (tool_name, config) in configs {
        if !config.enabled {
            continue;
        }
        if config.command == "sequence" {
            // Sequence tools orchestrate other tools and have no direct binary to probe.
            continue;
        }

        // Skip probe for project-relative commands without explicit availability_check
        // Commands like "./gradlew" are project-specific and shouldn't be probed globally
        if config.availability_check.is_none()
            && (config.command.starts_with("./") || config.command.starts_with("../"))
        {
            debug!(
                "Skipping availability probe for tool '{}' with project-relative command '{}'",
                tool_name, config.command
            );
            continue;
        }

        plans.push(build_tool_plan(tool_name, config, default_working_dir));

        if let Some(subcommands) = &config.subcommand {
            collect_subcommand_plans(
                tool_name,
                config,
                subcommands,
                &mut plans,
                Vec::new(),
                default_working_dir,
            );
        }
    }

    plans
}

fn collect_subcommand_plans(
    tool_name: &str,
    config: &ToolConfig,
    subcommands: &[SubcommandConfig],
    plans: &mut Vec<ProbePlan>,
    prefix: Vec<String>,
    default_working_dir: &Path,
) {
    for sub in subcommands {
        if !sub.enabled {
            continue;
        }
        let mut path = prefix.clone();
        path.push(sub.name.clone());

        if let Some(nested) = &sub.subcommand {
            collect_subcommand_plans(
                tool_name,
                config,
                nested,
                plans,
                path.clone(),
                default_working_dir,
            );
        } else {
            // Only create probe plans for subcommands with explicit availability checks
            // (either on the subcommand itself or inherited from the tool)
            if sub.availability_check.is_some() {
                plans.push(build_subcommand_plan(
                    tool_name,
                    config,
                    sub,
                    path,
                    default_working_dir,
                ));
            }
        }
    }
}

fn build_tool_plan(tool_name: &str, config: &ToolConfig, default_working_dir: &Path) -> ProbePlan {
    let (command, success_codes, working_dir) = resolve_command(
        config,
        None,
        config.availability_check.as_ref(),
        default_working_dir,
    );

    ProbePlan {
        target: ProbeTarget::Tool {
            name: tool_name.to_string(),
        },
        command,
        working_dir,
        success_codes,
        timeout_ms: config
            .timeout_seconds
            .map(|v| v * 1000)
            .unwrap_or(DEFAULT_PROBE_TIMEOUT_MS),
        install_instructions: config.install_instructions.clone(),
    }
}

fn build_subcommand_plan(
    tool_name: &str,
    config: &ToolConfig,
    sub: &SubcommandConfig,
    path: Vec<String>,
    default_working_dir: &Path,
) -> ProbePlan {
    let (command, success_codes, working_dir) = resolve_command(
        config,
        Some((&path, sub)),
        sub.availability_check
            .as_ref()
            .or(config.availability_check.as_ref()),
        default_working_dir,
    );

    ProbePlan {
        target: ProbeTarget::Subcommand {
            tool: tool_name.to_string(),
            path,
        },
        command,
        working_dir,
        success_codes,
        timeout_ms: sub
            .timeout_seconds
            .or(config.timeout_seconds)
            .map(|v| v * 1000)
            .unwrap_or(DEFAULT_PROBE_TIMEOUT_MS),
        install_instructions: sub
            .install_instructions
            .clone()
            .or_else(|| config.install_instructions.clone()),
    }
}

fn resolve_command(
    config: &ToolConfig,
    sub_target: Option<(&[String], &SubcommandConfig)>,
    check: Option<&AvailabilityCheck>,
    default_working_dir: &Path,
) -> (Vec<String>, Vec<i32>, PathBuf) {
    let mut success_codes = vec![0];
    let mut working_dir = default_working_dir.to_path_buf();

    if let Some(check) = check {
        if let Some(codes) = &check.success_exit_codes {
            if !codes.is_empty() {
                success_codes = codes.clone();
            }
        }
        if let Some(dir) = &check.working_directory {
            working_dir = PathBuf::from(dir);
        }
    }

    let mut command = match check.and_then(|c| c.command.as_ref()) {
        Some(cmd) => split_command(cmd),
        None => split_command(&config.command),
    };

    let skip_sub_args = check.map(|c| c.skip_subcommand_args).unwrap_or(false);

    if let Some((path, sub_config)) = sub_target {
        // Only add subcommand name to the command if:
        // 1. skip_subcommand_args is false, AND
        // 2. The subcommand doesn't have its own explicit availability_check
        //    (if it does, we use that check's command and args directly)
        let has_own_check = sub_config.availability_check.is_some();

        if !skip_sub_args && !has_own_check {
            for segment in path {
                if !segment.is_empty() {
                    command.push(segment.clone());
                }
            }
        }
        match check {
            Some(probe) => {
                command.extend(probe.args.clone());
            }
            None => {
                command.push("--help".to_string());
            }
        }
    } else if let Some(probe) = check {
        command.extend(probe.args.clone());
    } else {
        let base = command
            .first()
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| config.name.clone());
        command = vec!["/usr/bin/env".to_string(), "which".to_string(), base];
    }

    (command, success_codes, working_dir)
}

fn split_command(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

async fn run_probe(shell_pool: Arc<ShellPoolManager>, plan: ProbePlan) -> ProbeOutcome {
    let mut response = None;

    if let Some(mut shell) = shell_pool.get_shell(&plan.working_dir).await {
        let shell_command = ShellCommand {
            id: format!("availability-{:?}", plan.target),
            command: plan.command.clone(),
            working_dir: plan.working_dir.to_string_lossy().to_string(),
            timeout_ms: plan.timeout_ms,
        };

        match shell.execute_command(shell_command).await {
            Ok(resp) => {
                response = Some(resp);
            }
            Err(err) => {
                warn!("Shell execution failed for {:?}: {err}", plan.target);
            }
        }

        shell_pool.return_shell(shell).await;
    }

    let result = match response {
        Some(resp) => resp,
        None => execute_direct(&plan).await,
    };

    let success = plan.success_codes.contains(&result.exit_code);

    ProbeOutcome {
        plan,
        success,
        exit_code: Some(result.exit_code),
        stdout: result.stdout,
        stderr: result.stderr,
    }
}

async fn execute_direct(plan: &ProbePlan) -> crate::shell_pool::ShellResponse {
    let mut cmd_iter = plan.command.iter();
    let program = cmd_iter
        .next()
        .cloned()
        .unwrap_or_else(|| "true".to_string());
    let args: Vec<String> = cmd_iter.cloned().collect();

    let mut command = tokio::process::Command::new(&program);
    command.args(&args).current_dir(&plan.working_dir);

    let timeout_duration = Duration::from_millis(plan.timeout_ms);
    let result = timeout(timeout_duration, command.output()).await;

    let (exit_code, stdout, stderr) = match result {
        Ok(Ok(output)) => (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ),
        Ok(Err(err)) => (-1, String::new(), err.to_string()),
        Err(_) => (
            -1,
            String::new(),
            format!("probe timed out after {}ms", plan.timeout_ms),
        ),
    };

    crate::shell_pool::ShellResponse {
        id: format!("direct-availability-{:?}", plan.target),
        exit_code,
        stdout,
        stderr,
        duration_ms: 0,
    }
}

fn find_subcommand_mut<'a>(
    subcommands: &'a mut Option<Vec<SubcommandConfig>>,
    path: &[String],
) -> Option<&'a mut SubcommandConfig> {
    let list = subcommands.as_mut()?;
    find_subcommand_mut_in(list.as_mut_slice(), path)
}

fn find_subcommand_mut_in<'a>(
    subcommands: &'a mut [SubcommandConfig],
    path: &[String],
) -> Option<&'a mut SubcommandConfig> {
    if path.is_empty() {
        return None;
    }

    let (segment, rest) = path.split_first()?;
    for sub in subcommands.iter_mut() {
        if sub.name == *segment {
            if rest.is_empty() {
                return Some(sub);
            }
            if let Some(children) = sub.subcommand.as_mut() {
                if let Some(found) = find_subcommand_mut_in(children.as_mut_slice(), rest) {
                    return Some(found);
                }
            }
        }
    }
    None
}
