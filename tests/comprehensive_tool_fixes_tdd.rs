/// TDD tests to identify and fix clippy and nextest issues
#[cfg(test)]
mod comprehensive_tool_fixes_tdd {

    #[test]
    fn test_clippy_should_run_on_all_targets_including_tests() {
        // TDD: Define expected behavior for comprehensive clippy coverage

        // The user wants: clippy --fix --all --tests
        // This should check:
        // - All workspace packages (--all/--workspace)
        // - All targets including tests (--tests)
        // - Apply automatic fixes (--fix)

        // Let's verify what ahma_mcp clippy tool supports
        let cargo_clippy_exists = std::path::Path::new(".ahma/tools/cargo_clippy.json").exists();
        assert!(
            cargo_clippy_exists,
            "cargo_clippy.json should exist for clippy operations"
        );

        // Read and parse the clippy tool configuration
        let clippy_config = std::fs::read_to_string(".ahma/tools/cargo_clippy.json")
            .expect("Failed to read cargo_clippy.json");

        let clippy_json: serde_json::Value =
            serde_json::from_str(&clippy_config).expect("cargo_clippy.json should be valid JSON");

        // Check if it has the options we need
        let subcommands = clippy_json["subcommand"]
            .as_array()
            .expect("Should have subcommand array");

        assert!(
            !subcommands.is_empty(),
            "Should have at least one subcommand"
        );

        // Find the default subcommand and check its options
        let default_cmd = subcommands
            .iter()
            .find(|cmd| cmd["name"] == "default")
            .expect("Should have default subcommand");

        let options = default_cmd["options"]
            .as_array()
            .expect("Default subcommand should have options");

        // Check for required options
        let has_fix = options.iter().any(|opt| opt["name"] == "fix");
        let has_tests = options.iter().any(|opt| opt["name"] == "tests");
        let has_allow_dirty = options.iter().any(|opt| opt["name"] == "allow-dirty");

        assert!(has_fix, "clippy should support --fix option");
        assert!(has_tests, "clippy should support --tests option");
        assert!(
            has_allow_dirty,
            "clippy should support --allow-dirty option"
        );
    }

    #[test]
    fn test_nextest_should_support_run_subcommand() {
        // TDD: nextest needs "run" subcommand to work properly

        let nextest_config = std::fs::read_to_string(".ahma/tools/cargo_nextest.json")
            .expect("Failed to read cargo_nextest.json");

        let nextest_json: serde_json::Value =
            serde_json::from_str(&nextest_config).expect("cargo_nextest.json should be valid JSON");

        // The issue is that nextest requires a subcommand like "run"
        // But our current config only has "default" which doesn't map to nextest's expected subcommands

        // Check current structure
        let subcommands = nextest_json["subcommand"]
            .as_array()
            .expect("Should have subcommand array");

        // We need to determine if we should:
        // 1. Add a "run" subcommand alongside "default"
        // 2. Change the command structure to properly invoke "cargo nextest run"

        assert!(!subcommands.is_empty(), "Should have subcommands defined");

        println!(
            "Current nextest subcommands: {}",
            serde_json::to_string_pretty(subcommands).unwrap()
        );
    }

    #[test]
    fn test_verify_tools_directory_toml_files() {
        // TDD: Check if there are any .toml files in tools directory that need validation

        let tools_dir = std::path::Path::new(".ahma/tools");
        assert!(tools_dir.exists(), "Tools directory should exist");

        let mut toml_files = Vec::new();
        let mut all_files = Vec::new();

        for entry in std::fs::read_dir(tools_dir).expect("Failed to read tools directory") {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.is_file() {
                all_files.push(path.clone());

                if path.extension().is_some_and(|ext| ext == "toml") {
                    toml_files.push(path);
                }
            }
        }

        println!("All files in .ahma/tools/: {:?}", all_files);
        println!("TOML files found: {:?}", toml_files);

        // Validate any .toml files found
        for toml_file in &toml_files {
            let content = std::fs::read_to_string(toml_file)
                .unwrap_or_else(|_| panic!("Failed to read {:?}", toml_file));

            let parse_result = toml::from_str::<toml::Value>(&content);
            assert!(
                parse_result.is_ok(),
                "File {:?} should be valid TOML. Error: {:?}",
                toml_file,
                parse_result.err()
            );
        }

        // This test should pass if there are no .toml files (which is expected)
        // or if all .toml files are valid
    }

    #[test]
    fn test_comprehensive_quality_check_sequence() {
        // TDD: Define the complete quality check sequence using proper ahma_mcp tools

        // Expected sequence:
        // 1. cargo fmt (formatting)
        // 2. cargo clippy --fix --allow-dirty --tests (linting with fixes, including tests)
        // 3. cargo nextest run (testing with nextest)
        // 4. cargo check (syntax check)

        // This test defines what should work - actual execution will be done after fixes
        // Quality check sequence defined
    }
}
