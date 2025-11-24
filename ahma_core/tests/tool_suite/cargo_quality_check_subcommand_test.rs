/// TDD tests for the cargo qualitycheck subcommand sequence.
use ahma_core::config::{SubcommandConfig, ToolConfig};
use serde_json::Value;
use std::path::PathBuf;

fn load_cargo_config() -> ToolConfig {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .join(".ahma/tools/cargo.json");

    let json_content = std::fs::read_to_string(&config_path).expect("Failed to read cargo.json");

    serde_json::from_str(&json_content).expect("Failed to parse cargo.json")
}

fn find_qualitycheck_subcommand(config: &ToolConfig) -> &SubcommandConfig {
    config
        .subcommand
        .as_ref()
        .expect("cargo.json should have subcommands")
        .iter()
        .find(|s| s.name == "qualitycheck")
        .expect("cargo.json should have qualitycheck subcommand")
}

#[test]
fn test_cargo_qualitycheck_subcommand_structure() {
    let config = load_cargo_config();
    let qualitycheck = find_qualitycheck_subcommand(&config);

    assert_eq!(
        qualitycheck.name, "qualitycheck",
        "Subcommand should be named qualitycheck (no underscore)"
    );

    // Subcommand sequences are defined at the subcommand level
    assert!(
        qualitycheck.sequence.is_some(),
        "qualitycheck must define a subcommand-level sequence"
    );

    assert_eq!(
        qualitycheck.force_synchronous,
        Some(true),
        "Sequence should run synchronously"
    );

    assert_eq!(
        qualitycheck.step_delay_ms,
        Some(500),
        "Sequence should enforce a delay between steps"
    );
}

#[test]
fn test_cargo_qualitycheck_sequence_steps() {
    let config = load_cargo_config();
    let qualitycheck = find_qualitycheck_subcommand(&config);

    let sequence = qualitycheck
        .sequence
        .as_ref()
        .expect("qualitycheck must contain a subcommand-level sequence");

    // qualitycheck is generic and should have exactly 5 steps:
    // fmt, clippy (code), clippy (tests), nextest_run, build
    assert_eq!(
        sequence.len(),
        5,
        "Generic qualitycheck should have 5 steps (no schema generation or validation)"
    );

    // Should NOT have schema generation or validation (those are in ahma_quality_check)
    let has_generate_schema = sequence.iter().any(|step| {
        step.subcommand == "run"
            && step
                .args
                .get("bin")
                .is_some_and(|value| value == &Value::String("generate_tool_schema".into()))
    });
    assert!(
        !has_generate_schema,
        "Generic qualitycheck should NOT regenerate the schema"
    );

    let has_validate = sequence.iter().any(|step| {
        step.subcommand == "run"
            && step
                .args
                .get("bin")
                .is_some_and(|value| value == &Value::String("ahma_validate".into()))
    });
    assert!(
        !has_validate,
        "Generic qualitycheck should NOT validate tool configurations"
    );

    // Should have the standard Rust quality check steps
    assert!(
        sequence.iter().any(|step| step.subcommand == "fmt"),
        "Sequence should format the workspace"
    );
    assert!(
        sequence
            .iter()
            .filter(|step| step.subcommand == "clippy")
            .count()
            >= 2,
        "Sequence should lint both code and tests"
    );
    assert!(
        sequence.iter().any(|step| step.subcommand == "nextest_run"),
        "Sequence should execute the nextest suite"
    );
    assert!(
        sequence.iter().any(|step| step.subcommand == "build"),
        "Sequence should finish with a build"
    );
}
