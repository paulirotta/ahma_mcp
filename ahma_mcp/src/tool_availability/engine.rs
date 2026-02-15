use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tracing::debug;

use crate::config::ToolConfig;
use crate::shell_pool::ShellPoolManager;

use super::builder::build_probe_plans;
use super::types::{ProbeOutcome, ProbePlan};
use super::{AvailabilitySummary, DisabledSubcommand, DisabledTool};

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
