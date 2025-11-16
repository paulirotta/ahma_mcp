use ahma_core::config::{AvailabilityCheck, SubcommandConfig, ToolConfig, ToolHints};
use ahma_core::shell_pool::{ShellPoolConfig, ShellPoolManager};
use ahma_core::tool_availability::{
    AvailabilitySummary, DisabledSubcommand, DisabledTool, evaluate_tool_availability,
    format_install_guidance,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn build_subcommand(
    name: &str,
    check: Option<AvailabilityCheck>,
    install: Option<&str>,
) -> SubcommandConfig {
    SubcommandConfig {
        name: name.to_string(),
        description: format!("{} subcommand", name),
        options: None,
        positional_args: None,
        force_synchronous: None,
        timeout_seconds: None,
        enabled: true,
        guidance_key: None,
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        availability_check: check,
        install_instructions: install.map(|s| s.to_string()),
    }
}

fn base_tool(command: &str) -> ToolConfig {
    ToolConfig {
        name: format!("{}_tool", command.replace(' ', "_")),
        description: format!("{} wrapper", command),
        command: command.to_string(),
        subcommand: None,
        sequence: None,
        step_delay_ms: None,
        input_schema: None,
        timeout_seconds: Some(5),
        force_synchronous: None,
        hints: ToolHints::default(),
        enabled: true,
        guidance_key: None,
        availability_check: None,
        install_instructions: None,
    }
}

#[tokio::test]
async fn test_availability_disables_missing_tools_and_subcommands() -> Result<()> {
    let shell_pool = Arc::new(ShellPoolManager::new(ShellPoolConfig::default()));

    // Tool with missing binary
    let mut missing_tool = base_tool("nonexistent_binary_for_mcp");
    missing_tool.name = "missing_tool".to_string();
    missing_tool.install_instructions = Some("brew install imaginary-tool".to_string());

    // Tool with two subcommands: one ok, one broken
    let mut multi_tool = base_tool("bash");
    multi_tool.name = "multi_tool".to_string();
    multi_tool.availability_check = Some(AvailabilityCheck {
        command: Some("bash".to_string()),
        args: vec!["-lc".to_string(), "exit 0".to_string()],
        ..Default::default()
    });
    multi_tool.subcommand = Some(vec![
        build_subcommand(
            "available",
            Some(AvailabilityCheck {
                command: Some("bash".to_string()),
                args: vec!["-lc".to_string(), "exit 0".to_string()],
                ..Default::default()
            }),
            None,
        ),
        build_subcommand(
            "broken",
            Some(AvailabilityCheck {
                command: Some("bash".to_string()),
                args: vec!["-lc".to_string(), "exit 42".to_string()],
                ..Default::default()
            }),
            Some("rustup component add foo"),
        ),
    ]);

    let mut configs: HashMap<String, ToolConfig> = HashMap::new();
    configs.insert(missing_tool.name.clone(), missing_tool);
    configs.insert(multi_tool.name.clone(), multi_tool);

    let summary: AvailabilitySummary =
        evaluate_tool_availability(shell_pool, configs, Path::new(".")).await?;

    // Missing tool must be disabled with install instructions
    let disabled_names: Vec<_> = summary
        .disabled_tools
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert!(disabled_names.contains(&"missing_tool"));
    let missing_config = summary
        .filtered_configs
        .get("missing_tool")
        .expect("missing_tool config should remain present for diagnostics");
    assert!(
        !missing_config.enabled,
        "missing tool should be disabled at runtime"
    );

    // Multi tool remains enabled but broken subcommand should be disabled
    let multi_config = summary
        .filtered_configs
        .get("multi_tool")
        .expect("multi_tool should be retained");
    assert!(
        multi_config.enabled,
        "multi_tool itself should stay enabled"
    );

    let subcommands = multi_config
        .subcommand
        .as_ref()
        .expect("multi_tool should keep subcommands");
    let available = subcommands
        .iter()
        .find(|s| s.name == "available")
        .expect("available subcommand should exist");
    assert!(
        available.enabled,
        "available subcommand should remain enabled"
    );

    let broken = subcommands
        .iter()
        .find(|s| s.name == "broken")
        .expect("broken subcommand should still be present");
    assert!(
        !broken.enabled,
        "broken subcommand must be disabled after failed probe"
    );

    let disabled_sub_paths: Vec<_> = summary
        .disabled_subcommands
        .iter()
        .map(|d| format!("{}::{}", d.tool, d.subcommand_path))
        .collect();
    assert!(
        disabled_sub_paths.contains(&"multi_tool::broken".to_string()),
        "Disabled subcommand should be reported"
    );

    Ok(())
}

#[test]
fn test_install_guidance_formatter_includes_all_hints() {
    let summary = AvailabilitySummary {
        filtered_configs: HashMap::new(),
        disabled_tools: vec![DisabledTool {
            name: "missing_tool".to_string(),
            message: "binary not found".to_string(),
            install_instructions: Some("brew install missing-tool".to_string()),
        }],
        disabled_subcommands: vec![DisabledSubcommand {
            tool: "cargo".to_string(),
            subcommand_path: "nextest_run".to_string(),
            message: "cargo nextest not installed".to_string(),
            install_instructions: Some("cargo install cargo-nextest".to_string()),
        }],
    };

    let guidance = format_install_guidance(&summary);

    assert!(
        guidance.contains("missing_tool"),
        "tool guidance should list missing_tool"
    );
    assert!(
        guidance.contains("brew install missing-tool"),
        "tool guidance should contain install hint"
    );
    assert!(
        guidance.contains("cargo::nextest_run"),
        "subcommand guidance should include fully qualified name"
    );
    assert!(
        guidance.contains("cargo install cargo-nextest"),
        "subcommand guidance should contain nextest install hint"
    );
}
