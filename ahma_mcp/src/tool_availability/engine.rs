use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::{AvailabilityCheck, SubcommandConfig, ToolConfig};
use crate::shell_pool::{ShellCommand, ShellPoolManager};

use super::{AvailabilitySummary, DisabledSubcommand, DisabledTool};

const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;

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

pub(super) async fn evaluate_tool_availability_impl(
    shell_pool: Arc<ShellPoolManager>,
    configs: HashMap<String, ToolConfig>,
    default_working_dir: &Path,
    sandbox: &crate::sandbox::Sandbox,
) -> Result<AvailabilitySummary> {
    // If a sandbox scope is initialized, prefer it for availability probes.
    // This matters on Linux Landlock where the process current working directory may be outside
    // the enforced scope (e.g., integration tests start the server from the repo root but set
    // sandbox scope to a temp directory).
    let sandbox_default = sandbox.scopes().first().cloned();
    let default_working_dir = sandbox_default.as_deref().unwrap_or(default_working_dir);

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

    let outcomes = execute_probes(shell_pool, plans).await;
    let (disabled_tools, disabled_subcommands) =
        process_probe_outcomes(outcomes, &mut filtered_configs);

    Ok(AvailabilitySummary {
        filtered_configs,
        disabled_tools,
        disabled_subcommands,
    })
}

fn process_probe_outcomes(
    outcomes: Vec<ProbeOutcome>,
    filtered_configs: &mut HashMap<String, ToolConfig>,
) -> (Vec<DisabledTool>, Vec<DisabledSubcommand>) {
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
                display_output(stdout),
                display_output(stderr),
            );
            continue;
        }

        match &outcome.plan.target {
            ProbeTarget::Tool { name } => {
                disable_tool(
                    name,
                    &outcome.plan,
                    outcome.exit_code,
                    stdout,
                    stderr,
                    filtered_configs,
                    &mut disabled_tools,
                );
            }
            ProbeTarget::Subcommand { tool, path } => {
                disable_subcommand(
                    tool,
                    path,
                    &outcome.plan,
                    outcome.exit_code,
                    stdout,
                    stderr,
                    filtered_configs,
                    &mut disabled_subcommands,
                );
            }
        }
    }

    (disabled_tools, disabled_subcommands)
}

fn disable_tool(
    name: &str,
    plan: &ProbePlan,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    filtered_configs: &mut HashMap<String, ToolConfig>,
    disabled_tools: &mut Vec<DisabledTool>,
) {
    if let Some(config) = filtered_configs.get_mut(name) {
        config.enabled = false;
    }
    let label = format!("Tool '{}'", name);
    let message = build_failure_message(&label, &plan.command, exit_code, stdout, stderr);
    log_probe_failure(name, &message, plan.install_instructions.as_deref());
    disabled_tools.push(DisabledTool {
        name: name.to_string(),
        message,
        install_instructions: plan.install_instructions.clone(),
    });
}

fn disable_subcommand(
    tool: &str,
    path: &[String],
    plan: &ProbePlan,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    filtered_configs: &mut HashMap<String, ToolConfig>,
    disabled_subcommands: &mut Vec<DisabledSubcommand>,
) {
    if let Some(config) = filtered_configs.get_mut(tool)
        && let Some(sub) = find_subcommand_mut(&mut config.subcommand, path)
    {
        sub.enabled = false;
    }

    let joined_path = path.join("_");
    let label = format!("Subcommand '{}::{}'", tool, joined_path);
    let message = build_failure_message(&label, &plan.command, exit_code, stdout, stderr);
    let install_label = format!("{}::{}", tool, joined_path);
    log_probe_failure(
        &install_label,
        &message,
        plan.install_instructions.as_deref(),
    );
    disabled_subcommands.push(DisabledSubcommand {
        tool: tool.to_string(),
        subcommand_path: joined_path,
        message,
        install_instructions: plan.install_instructions.clone(),
    });
}

async fn execute_probes(
    shell_pool: Arc<ShellPoolManager>,
    plans: Vec<ProbePlan>,
) -> Vec<ProbeOutcome> {
    let probe_tasks: Vec<_> = plans
        .into_iter()
        .map(|plan| {
            let shell_pool = shell_pool.clone();
            tokio::spawn(async move { run_probe(shell_pool, plan).await })
        })
        .collect();

    futures::future::join_all(probe_tasks)
        .await
        .into_iter()
        .filter_map(|result| result.ok())
        .collect()
}

async fn run_probe(shell_pool: Arc<ShellPoolManager>, plan: ProbePlan) -> ProbeOutcome {
    let result = match try_shell_execution(&shell_pool, &plan).await {
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

/// Attempt to execute a probe via the shell pool; returns `None` if no shell
/// was available or execution failed.
async fn try_shell_execution(
    shell_pool: &ShellPoolManager,
    plan: &ProbePlan,
) -> Option<crate::shell_pool::ShellResponse> {
    let mut shell = shell_pool.get_shell(&plan.working_dir).await?;
    let shell_command = ShellCommand {
        id: format!("availability-{:?}", plan.target),
        command: plan.command.clone(),
        working_dir: plan.working_dir.to_string_lossy().to_string(),
        timeout_ms: plan.timeout_ms,
    };

    let response = shell.execute_command(shell_command).await;
    shell_pool.return_shell(shell).await;

    match response {
        Ok(resp) => Some(resp),
        Err(err) => {
            warn!("Shell execution failed for {:?}: {err}", plan.target);
            None
        }
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

fn build_probe_plans(
    configs: &HashMap<String, ToolConfig>,
    default_working_dir: &Path,
) -> Vec<ProbePlan> {
    let mut plans = Vec::new();

    for (tool_name, config) in configs {
        if !config.enabled || config.command == "sequence" {
            continue;
        }

        // Skip probe for project-relative commands without explicit availability_check.
        // Commands like "./gradlew" are project-specific and shouldn't be probed globally.
        if config.availability_check.is_none() && is_relative_command(&config.command) {
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

/// Returns `true` for project-relative commands (e.g. `"./gradlew"`, `"../bin/tool"`).
fn is_relative_command(command: &str) -> bool {
    command.starts_with("./") || command.starts_with("../")
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
        } else if sub.availability_check.is_some() {
            // Only create probe plans for leaf subcommands with explicit availability checks.
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
        if let Some(codes) = &check.success_exit_codes
            && !codes.is_empty()
        {
            success_codes = codes.clone();
        }
        if let Some(dir) = &check.working_directory {
            working_dir = PathBuf::from(dir);
        }
    }

    let command = construct_probe_command(config, sub_target, check);
    (command, success_codes, working_dir)
}

fn construct_probe_command(
    config: &ToolConfig,
    sub_target: Option<(&[String], &SubcommandConfig)>,
    check: Option<&AvailabilityCheck>,
) -> Vec<String> {
    let mut command = base_probe_command(config, check);

    match sub_target {
        Some((path, sub_config)) => {
            append_subcommand_probe_args(&mut command, path, sub_config, check);
        }
        None => {
            append_tool_probe_args(&mut command, config, check);
        }
    }

    command
}

/// Resolve the base command for a probe from the availability check or tool config.
fn base_probe_command(config: &ToolConfig, check: Option<&AvailabilityCheck>) -> Vec<String> {
    match check.and_then(|c| c.command.as_ref()) {
        Some(cmd) => split_command(cmd),
        None => split_command(&config.command),
    }
}

/// Append subcommand path segments and probe args to the command.
fn append_subcommand_probe_args(
    command: &mut Vec<String>,
    path: &[String],
    sub_config: &SubcommandConfig,
    check: Option<&AvailabilityCheck>,
) {
    let skip_sub_args = check.map(|c| c.skip_subcommand_args).unwrap_or(false);
    let has_own_check = sub_config.availability_check.is_some();

    if !skip_sub_args && !has_own_check {
        command.extend(path.iter().filter(|s| !s.is_empty()).cloned());
    }

    match check {
        Some(probe) => command.extend(probe.args.clone()),
        None => command.push("--help".to_string()),
    }
}

/// Append probe args for a tool-level (non-subcommand) probe.
fn append_tool_probe_args(
    command: &mut Vec<String>,
    config: &ToolConfig,
    check: Option<&AvailabilityCheck>,
) {
    match check {
        Some(probe) => command.extend(probe.args.clone()),
        None => {
            let base = command
                .first()
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| config.name.clone());
            *command = vec!["/usr/bin/env".to_string(), "which".to_string(), base];
        }
    }
}

fn split_command(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
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
            if let Some(children) = sub.subcommand.as_mut()
                && let Some(found) = find_subcommand_mut_in(children.as_mut_slice(), rest)
            {
                return Some(found);
            }
        }
    }
    None
}

fn display_output(s: &str) -> &str {
    if s.is_empty() { "<empty>" } else { s }
}

fn build_failure_message(
    label: &str,
    command: &[String],
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> String {
    format!(
        "{} disabled. Probe command {:?} failed with exit {:?}. stdout: {} stderr: {}",
        label,
        command,
        exit_code,
        display_output(stdout),
        display_output(stderr),
    )
}

fn log_probe_failure(label: &str, message: &str, install_instructions: Option<&str>) {
    warn!("{message}");
    if let Some(instructions) = install_instructions {
        info!("Install hint for '{}': {}", label, instructions.trim());
    }
}
