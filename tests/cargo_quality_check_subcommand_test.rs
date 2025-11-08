/// TDD test for cargo quality-check subcommand with sequence support
/// This test validates that cargo subcommands can include a "sequence" field
/// that executes multiple cargo commands in order.
use ahma_mcp::config::ToolConfig;
use std::path::PathBuf;

#[test]
fn test_cargo_quality_check_subcommand_loads_successfully() {
    // Arrange: Load the cargo.json tool configuration
    let cargo_json_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".ahma/tools/cargo.json");

    let json_content =
        std::fs::read_to_string(&cargo_json_path).expect("Failed to read cargo.json");

    // Act: Parse the configuration
    let result: Result<ToolConfig, _> = serde_json::from_str(&json_content);

    // Assert: Should parse without errors
    assert!(
        result.is_ok(),
        "Failed to parse cargo.json: {:?}",
        result.err()
    );

    let config = result.unwrap();

    // Assert: Should have quality-check subcommand
    assert!(
        config.subcommand.is_some(),
        "cargo.json should have subcommands"
    );
    let subcommands = config.subcommand.unwrap();

    let quality_check = subcommands
        .iter()
        .find(|sc| sc.name == "quality-check")
        .expect("Should have quality-check subcommand");

    // Assert: quality-check should have sequence field
    assert!(
        quality_check.sequence.is_some(),
        "quality-check subcommand should have a sequence field"
    );

    let sequence = quality_check.sequence.as_ref().unwrap();

    // Assert: sequence should have the expected steps
    assert!(
        sequence.len() >= 4,
        "quality-check should have at least 4 steps"
    );

    // Verify the sequence contains expected cargo commands
    let step_subcommands: Vec<&str> = sequence
        .iter()
        .map(|step| step.subcommand.as_str())
        .collect();

    assert!(step_subcommands.contains(&"fmt"), "Should include fmt step");
    assert!(
        step_subcommands.contains(&"clippy"),
        "Should include clippy step"
    );
    assert!(
        step_subcommands.contains(&"test"),
        "Should include test step"
    );
    assert!(
        step_subcommands.contains(&"build"),
        "Should include build step"
    );
}

#[test]
fn test_cargo_quality_check_sequence_execution() {
    use ahma_mcp::config::ToolConfig;
    use std::path::PathBuf;

    // Load the cargo.json tool configuration
    let cargo_json_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".ahma/tools/cargo.json");

    let json_content =
        std::fs::read_to_string(&cargo_json_path).expect("Failed to read cargo.json");

    let cargo_config: ToolConfig =
        serde_json::from_str(&json_content).expect("Failed to parse cargo.json");

    // Find the quality-check subcommand
    let subcommands = cargo_config.subcommand.expect("Should have subcommands");
    let quality_check = subcommands
        .iter()
        .find(|sc| sc.name == "quality-check")
        .expect("Should have quality-check subcommand");

    // Verify sequence structure
    let sequence = quality_check
        .sequence
        .as_ref()
        .expect("quality-check should have a sequence");

    // Verify all steps are properly configured
    for (i, step) in sequence.iter().enumerate() {
        assert_eq!(step.tool, "cargo", "Step {} should use cargo tool", i);
        assert!(
            !step.subcommand.is_empty(),
            "Step {} should have a subcommand",
            i
        );
        assert!(
            step.description.is_some(),
            "Step {} should have a description",
            i
        );
    }

    // Verify we have the expected number of steps
    assert!(
        sequence.len() >= 4,
        "Should have at least 4 quality check steps"
    );

    // Verify synchronous execution
    assert_eq!(
        quality_check.synchronous,
        Some(true),
        "quality-check should be synchronous"
    );

    // Verify timeout is set
    assert!(
        quality_check.timeout_seconds.is_some(),
        "quality-check should have a timeout"
    );

    println!("✓ Cargo quality-check subcommand sequence is properly configured");
}
