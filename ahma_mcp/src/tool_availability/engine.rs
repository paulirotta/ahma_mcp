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

enum ProbeCommandTarget<'a> {
    Tool,
    Subcommand {
        path: &'a [String],
        has_own_check: bool,
    },
}

impl ProbePlan {
    fn for_tool(tool_name: &str, config: &ToolConfig, default_working_dir: &Path) -> Self {
        let check = config.availability_check.as_ref();
        let command = Self::build_probe_command(config, check, ProbeCommandTarget::Tool);
        let success_codes = Self::resolve_success_codes(check);
        let working_dir = Self::resolve_working_dir(check, default_working_dir);

        Self {
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

    fn for_subcommand(
        tool_name: &str,
        config: &ToolConfig,
        sub: &SubcommandConfig,
        path: Vec<String>,
        default_working_dir: &Path,
    ) -> Self {
        let check = sub
            .availability_check
            .as_ref()
            .or(config.availability_check.as_ref());
        let command = Self::build_probe_command(
            config,
            check,
            ProbeCommandTarget::Subcommand {
                path: &path,
                has_own_check: sub.availability_check.is_some(),
            },
        );
        let success_codes = Self::resolve_success_codes(check);
        let working_dir = Self::resolve_working_dir(check, default_working_dir);

        Self {
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

    fn resolve_success_codes(check: Option<&AvailabilityCheck>) -> Vec<i32> {
        check
            .and_then(|probe| probe.success_exit_codes.clone())
            .filter(|codes| !codes.is_empty())
            .unwrap_or_else(|| vec![0])
    }

    fn resolve_working_dir(
        check: Option<&AvailabilityCheck>,
        default_working_dir: &Path,
    ) -> PathBuf {
        check
            .and_then(|probe| probe.working_directory.as_deref())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_working_dir.to_path_buf())
    }

    fn build_probe_command(
        config: &ToolConfig,
        check: Option<&AvailabilityCheck>,
        target: ProbeCommandTarget<'_>,
    ) -> Vec<String> {
        let mut command = match check.and_then(|probe| probe.command.as_ref()) {
            Some(cmd) => Self::split_command(cmd),
            None => Self::split_command(&config.command),
        };

        match target {
            ProbeCommandTarget::Tool => Self::apply_tool_probe_logic(&mut command, config, check),
            ProbeCommandTarget::Subcommand {
                path,
                has_own_check,
            } => Self::apply_subcommand_probe_logic(&mut command, check, path, has_own_check),
        }

        command
    }

    fn apply_tool_probe_logic(
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
                    .filter(|segment| !segment.is_empty())
                    .unwrap_or_else(|| config.name.clone());
                *command = vec!["/usr/bin/env".to_string(), "which".to_string(), base];
            }
        }
    }

    fn apply_subcommand_probe_logic(
        command: &mut Vec<String>,
        check: Option<&AvailabilityCheck>,
        path: &[String],
        has_own_check: bool,
    ) {
        let skip_subcommand_args = check
            .map(|probe| probe.skip_subcommand_args)
            .unwrap_or(false);
        if !skip_subcommand_args && !has_own_check {
            command.extend(path.iter().filter(|segment| !segment.is_empty()).cloned());
        }

        match check {
            Some(probe) => command.extend(probe.args.clone()),
            None => command.push("--help".to_string()),
        }
    }

    fn split_command(command: &str) -> Vec<String> {
        command
            .split_whitespace()
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect()
    }

    async fn execute(self, shell_pool: Arc<ShellPoolManager>) -> ProbeOutcome {
        let result = match self.try_shell(shell_pool.as_ref()).await {
            Some(response) => response,
            None => self.execute_direct().await,
        };
        let success = self.success_codes.contains(&result.exit_code);

        ProbeOutcome {
            plan: self,
            success,
            exit_code: Some(result.exit_code),
            stdout: result.stdout,
            stderr: result.stderr,
        }
    }

    /// Attempt to execute a probe via the shell pool; returns `None` if no shell
    /// was available or execution failed.
    async fn try_shell(
        &self,
        shell_pool: &ShellPoolManager,
    ) -> Option<crate::shell_pool::ShellResponse> {
        let mut shell = shell_pool.get_shell(&self.working_dir).await?;
        let shell_command = ShellCommand {
            id: format!("availability-{:?}", self.target),
            command: self.command.clone(),
            working_dir: self.working_dir.to_string_lossy().to_string(),
            timeout_ms: self.timeout_ms,
        };

        let response = shell.execute_command(shell_command).await;
        shell_pool.return_shell(shell).await;

        match response {
            Ok(resp) => Some(resp),
            Err(err) => {
                warn!("Shell execution failed for {:?}: {err}", self.target);
                None
            }
        }
    }

    async fn execute_direct(&self) -> crate::shell_pool::ShellResponse {
        let (program, args) = self.prepare_direct_command();

        let mut command = tokio::process::Command::new(&program);
        command.args(&args).current_dir(&self.working_dir);

        let timeout_duration = Duration::from_millis(self.timeout_ms);
        let result = timeout(timeout_duration, command.output()).await;

        let (exit_code, stdout, stderr) = self.process_direct_output(result);

        crate::shell_pool::ShellResponse {
            id: format!("direct-availability-{:?}", self.target),
            exit_code,
            stdout,
            stderr,
            duration_ms: 0,
        }
    }

    fn prepare_direct_command(&self) -> (String, Vec<String>) {
        let mut cmd_iter = self.command.iter();
        let program = cmd_iter
            .next()
            .cloned()
            .unwrap_or_else(|| "true".to_string());
        let args: Vec<String> = cmd_iter.cloned().collect();
        (program, args)
    }

    fn process_direct_output(
        &self,
        result: Result<
            std::result::Result<std::process::Output, std::io::Error>,
            tokio::time::error::Elapsed,
        >,
    ) -> (i32, String, String) {
        match result {
            Ok(Ok(output)) => (
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout).to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ),
            Ok(Err(err)) => (-1, String::new(), err.to_string()),
            Err(_) => (
                -1,
                String::new(),
                format!("probe timed out after {}ms", self.timeout_ms),
            ),
        }
    }
}

impl ProbeTarget {
    fn tool_name(&self) -> &str {
        match self {
            ProbeTarget::Tool { name } => name,
            ProbeTarget::Subcommand { tool, .. } => tool,
        }
    }

    fn label(&self) -> String {
        match self {
            ProbeTarget::Tool { name } => format!("Tool '{}'", name),
            ProbeTarget::Subcommand { tool, path } => {
                format!("Subcommand '{}::{}'", tool, path.join("_"))
            }
        }
    }

    fn install_label(&self) -> String {
        match self {
            ProbeTarget::Tool { name } => name.clone(),
            ProbeTarget::Subcommand { tool, path } => format!("{}::{}", tool, path.join("_")),
        }
    }
}

impl ProbeOutcome {
    fn update_config(&self, configs: &mut HashMap<String, ToolConfig>) {
        if self.success {
            return;
        }

        let tool_name = self.plan.target.tool_name();
        let config = match configs.get_mut(tool_name) {
            Some(c) => c,
            None => return,
        };

        match &self.plan.target {
            ProbeTarget::Tool { .. } => {
                config.enabled = false;
            }
            ProbeTarget::Subcommand { path, .. } => {
                if let Some(subcommands) = config.subcommand.as_mut()
                    && let Some(sub) = find_subcommand_mut_in(subcommands, path)
                {
                    sub.enabled = false;
                }
            }
        }
    }

    fn to_disabled_items(&self) -> (Option<DisabledTool>, Option<DisabledSubcommand>) {
        if self.success {
            return (None, None);
        }

        let label = self.plan.target.label();
        let install_label = self.plan.target.install_label();
        let message = self.log_and_build_message(&label, &install_label);
        let instructions = self.plan.install_instructions.clone();

        match &self.plan.target {
            ProbeTarget::Tool { name } => (
                Some(DisabledTool {
                    name: name.clone(),
                    message,
                    install_instructions: instructions,
                }),
                None,
            ),
            ProbeTarget::Subcommand {
                tool,
                path: _path, // path is unused here as we use helpers for labels
            } => (
                None,
                Some(DisabledSubcommand {
                    tool: tool.clone(),
                    subcommand_path: install_label
                        .strip_prefix(&format!("{}::", tool))
                        .unwrap_or(&install_label)
                        .to_string(),
                    message,
                    install_instructions: instructions,
                }),
            ),
        }
    }

    fn log_and_build_message(&self, label: &str, install_label: &str) -> String {
        let stdout = if self.stdout.trim().is_empty() {
            "<empty>"
        } else {
            self.stdout.trim()
        };
        let stderr = if self.stderr.trim().is_empty() {
            "<empty>"
        } else {
            self.stderr.trim()
        };

        let message = format!(
            "{} disabled. Probe command {:?} failed with exit {:?}. stdout: {} stderr: {}",
            label, self.plan.command, self.exit_code, stdout, stderr
        );
        warn!("{message}");
        if let Some(instructions) = self.plan.install_instructions.as_deref() {
            info!(
                "Install hint for '{}': {}",
                install_label,
                instructions.trim()
            );
        }
        message
    }

    fn log_success(&self) {
        if !self.success {
            return;
        }
        let stdout = self.stdout.trim();
        let stderr = self.stderr.trim();
        let stdout = if stdout.is_empty() { "<empty>" } else { stdout };
        let stderr = if stderr.is_empty() { "<empty>" } else { stderr };
        debug!(
            "Probe success for {:?} (exit {:?}) stdout: {} stderr: {}",
            self.plan.target, self.exit_code, stdout, stderr,
        );
    }
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
    // 1. Log successes and filter for failures
    let failures: Vec<_> = outcomes
        .into_iter()
        .filter(|outcome| {
            if outcome.success {
                outcome.log_success();
                false
            } else {
                true
            }
        })
        .collect();

    // 2. Update configs for failures
    for failure in &failures {
        failure.update_config(filtered_configs);
    }

    // 3. Map failures to disabled items
    failures
        .into_iter()
        .map(|failure| failure.to_disabled_items())
        .fold(
            (Vec::new(), Vec::new()),
            |(mut tools, mut subs), (tool, sub)| {
                if let Some(t) = tool {
                    tools.push(t);
                }
                if let Some(s) = sub {
                    subs.push(s);
                }
                (tools, subs)
            },
        )
}

async fn execute_probes(
    shell_pool: Arc<ShellPoolManager>,
    plans: Vec<ProbePlan>,
) -> Vec<ProbeOutcome> {
    let probe_tasks: Vec<_> = plans
        .into_iter()
        .map(|plan| {
            let shell_pool = shell_pool.clone();
            tokio::spawn(async move { plan.execute(shell_pool).await })
        })
        .collect();

    futures::future::join_all(probe_tasks)
        .await
        .into_iter()
        .filter_map(|result| result.ok())
        .collect()
}

fn build_probe_plans(
    configs: &HashMap<String, ToolConfig>,
    default_working_dir: &Path,
) -> Vec<ProbePlan> {
    let mut plans = Vec::new();

    for (tool_name, config) in configs {
        if !should_probe_tool(tool_name, config) {
            continue;
        }

        plans.push(ProbePlan::for_tool(tool_name, config, default_working_dir));

        if let Some(subcommands) = &config.subcommand {
            collect_subcommand_plans(
                &mut plans,
                tool_name,
                config,
                subcommands,
                Vec::new(),
                default_working_dir,
            );
        }
    }

    plans
}

fn should_probe_tool(tool_name: &str, config: &ToolConfig) -> bool {
    if !config.enabled || config.command == "sequence" {
        return false;
    }

    // Skip probe for project-relative commands without explicit availability_check.
    // Commands like "./gradlew" are project-specific and shouldn't be probed globally.
    if config.availability_check.is_none()
        && (config.command.starts_with("./") || config.command.starts_with("../"))
    {
        debug!(
            "Skipping availability probe for tool '{}' with project-relative command '{}'",
            tool_name, config.command
        );
        return false;
    }

    true
}
fn collect_subcommand_plans(
    plans: &mut Vec<ProbePlan>,
    tool_name: &str,
    config: &ToolConfig,
    subcommands: &[SubcommandConfig],
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
                plans,
                tool_name,
                config,
                nested,
                path.clone(),
                default_working_dir,
            );
        } else if sub.availability_check.is_some() {
            // Only create probe plans for leaf subcommands with explicit availability checks.
            plans.push(ProbePlan::for_subcommand(
                tool_name,
                config,
                sub,
                path,
                default_working_dir,
            ));
        }
    }
}

fn find_subcommand_mut_in<'a>(
    subcommands: &'a mut [SubcommandConfig],
    path: &[String],
) -> Option<&'a mut SubcommandConfig> {
    let (segment, rest) = path.split_first()?;
    let sub = subcommands.iter_mut().find(|s| s.name == *segment)?;

    if rest.is_empty() {
        return Some(sub);
    }

    let children = sub.subcommand.as_mut()?;
    find_subcommand_mut_in(children.as_mut_slice(), rest)
}
