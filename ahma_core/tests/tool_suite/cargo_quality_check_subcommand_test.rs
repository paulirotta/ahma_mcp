/// TDD tests for the dedicated rust_quality_check sequence tool.
use ahma_core::config::ToolConfig;
use serde_json::Value;
use std::path::PathBuf;

fn load_rust_quality_check_config() -> ToolConfig {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .join(".ahma/tools/rust_quality_check.json");

    let json_content =
        std::fs::read_to_string(&config_path).expect("Failed to read rust_quality_check.json");

    serde_json::from_str(&json_content).expect("Failed to parse rust_quality_check.json")
}

#[test]
fn test_rust_quality_check_tool_structure() {
    let config = load_rust_quality_check_config();

    assert_eq!(
        config.command, "sequence",
        "Tool should be a sequence executor"
    );

    // Sequence tools use top-level sequence, not subcommand-level
    assert!(
        config.sequence.is_some(),
        "rust_quality_check must define a top-level sequence"
    );

    assert_eq!(
        config.force_synchronous,
        Some(true),
        "Sequence should run synchronously (asynchronous: false)"
    );

    assert_eq!(
        config.step_delay_ms,
        Some(500),
        "Sequence should enforce a delay between steps"
    );
}

#[test]
fn test_rust_quality_check_sequence_steps() {
    let config = load_rust_quality_check_config();

    let sequence = config
        .sequence
        .as_ref()
        .expect("rust_quality_check must contain a top-level sequence");

    assert!(
        sequence.len() >= 6,
        "Sequence should include the full pipeline"
    );

    let has_generate_schema = sequence.iter().any(|step| {
        step.subcommand == "run"
            && step
                .args
                .get("bin")
                .is_some_and(|value| value == &Value::String("generate_tool_schema".into()))
    });
    assert!(has_generate_schema, "Sequence should regenerate the schema");

    let has_validate = sequence.iter().any(|step| {
        step.subcommand == "run"
            && step
                .args
                .get("bin")
                .is_some_and(|value| value == &Value::String("ahma_validate".into()))
    });
    assert!(has_validate, "Sequence should validate tool configurations");

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
