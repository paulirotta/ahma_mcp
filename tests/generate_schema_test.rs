#[cfg(test)]
mod generate_schema_tests {
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn test_generate_schema_script_runs_successfully() {
        let temp_dir = tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");

        let output = Command::new("cargo")
            .args([
                "run",
                "--package",
                "ahma_mcp",
                "--bin",
                "generate_schema",
                "--",
                docs_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute generate_schema script");

        assert!(
            output.status.success(),
            "generate_schema script failed to run. Stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let schema_path = docs_dir.join("mtdf-schema.json");
        assert!(schema_path.exists(), "mtdf-schema.json was not created");

        let schema_content = fs::read_to_string(schema_path).unwrap();
        assert!(!schema_content.is_empty(), "mtdf-schema.json is empty");
        assert!(
            schema_content.contains("\"title\": \"ToolConfig\""),
            "Schema title is incorrect"
        );
    }

    #[test]
    fn test_generate_schema_default_docs_directory() {
        let output = Command::new("cargo")
            .args(["run", "--package", "ahma_mcp", "--bin", "generate_schema"])
            .output()
            .expect("Failed to execute generate_schema script");

        assert!(
            output.status.success(),
            "generate_schema script failed when using default docs directory. Stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify success message appears in stdout
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("✓ Generated MTDF JSON Schema"),
            "Missing success message"
        );
        assert!(stdout.contains("Schema size:"), "Missing schema size info");
        assert!(stdout.contains("Preview:"), "Missing preview section");
    }

    #[test]
    fn test_generate_schema_creates_nested_directories() {
        let temp_dir = tempdir().unwrap();
        let nested_dir = temp_dir.path().join("deeply").join("nested").join("path");

        let output = Command::new("cargo")
            .args([
                "run",
                "--package",
                "ahma_mcp",
                "--bin",
                "generate_schema",
                "--",
                nested_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute generate_schema script");

        assert!(
            output.status.success(),
            "generate_schema script failed to create nested directories. Stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let schema_path = nested_dir.join("mtdf-schema.json");
        assert!(
            schema_path.exists(),
            "mtdf-schema.json was not created in nested directory"
        );
        assert!(nested_dir.exists(), "Nested directory was not created");
    }

    #[test]
    fn test_schema_content_structure() {
        let temp_dir = tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");

        let output = Command::new("cargo")
            .args([
                "run",
                "--package",
                "ahma_mcp",
                "--bin",
                "generate_schema",
                "--",
                docs_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute generate_schema script");

        assert!(output.status.success());

        let schema_path = docs_dir.join("mtdf-schema.json");
        let schema_content = fs::read_to_string(schema_path).unwrap();

        // Parse as JSON to verify it's valid
        let schema: serde_json::Value =
            serde_json::from_str(&schema_content).expect("Schema file does not contain valid JSON");

        // Verify key schema properties
        assert!(
            schema.get("$schema").is_some(),
            "Schema missing $schema property"
        );
        assert!(
            schema.get("title").is_some(),
            "Schema missing title property"
        );
        assert!(schema.get("type").is_some(), "Schema missing type property");

        // Should have properties for ToolConfig fields
        if let Some(properties) = schema.get("properties") {
            assert!(properties.is_object(), "Properties should be an object");
            let props = properties.as_object().unwrap();
            assert!(!props.is_empty(), "Properties should not be empty");
        }
    }

    #[test]
    fn test_schema_output_format() {
        let temp_dir = tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");

        let output = Command::new("cargo")
            .args([
                "run",
                "--package",
                "ahma_mcp",
                "--bin",
                "generate_schema",
                "--",
                docs_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute generate_schema script");

        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should contain success indicator
        assert!(stdout.contains("✓"), "Missing success checkmark");

        // Should show file path
        let expected_path = docs_dir.join("mtdf-schema.json");
        assert!(
            stdout.contains(&expected_path.to_string_lossy().to_string()),
            "Missing output file path"
        );

        // Should show schema size
        assert!(
            stdout.contains("Schema size:") && stdout.contains("bytes"),
            "Missing schema size information"
        );

        // Should show preview
        assert!(stdout.contains("Preview:"), "Missing preview section");

        // Preview should show JSON lines
        assert!(stdout.contains("{"), "Preview should contain JSON content");
    }

    #[test]
    fn test_schema_file_size_reasonable() {
        let temp_dir = tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");

        let output = Command::new("cargo")
            .args([
                "run",
                "--package",
                "ahma_mcp",
                "--bin",
                "generate_schema",
                "--",
                docs_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute generate_schema script");

        assert!(output.status.success());

        let schema_path = docs_dir.join("mtdf-schema.json");
        let schema_content = fs::read_to_string(schema_path).unwrap();

        // Schema should be substantial but not enormous
        assert!(
            schema_content.len() > 500,
            "Schema seems too small, might be incomplete"
        );
        assert!(
            schema_content.len() < 100_000,
            "Schema seems unreasonably large"
        );

        // Should be properly formatted JSON (pretty printed)
        assert!(
            schema_content.contains('\n'),
            "Schema should be pretty-printed with newlines"
        );
        assert!(schema_content.contains("  "), "Schema should be indented");
    }
}
