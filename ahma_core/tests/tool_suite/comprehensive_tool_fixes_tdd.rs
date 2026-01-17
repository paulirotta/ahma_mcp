/// TDD tests to identify and fix clippy and nextest issues
#[cfg(test)]
#[allow(clippy::module_inception)]
mod comprehensive_tool_fixes_tdd {
    use ahma_core::test_utils::{get_tools_dir, get_workspace_path};

    #[test]
    fn test_clippy_should_run_on_all_targets_including_tests() {
        // TDD: Define expected behavior for comprehensive clippy coverage

        // The user wants: clippy --fix --all --tests
        // This should check:
        // - All workspace packages (--all/--workspace)
        // - All targets including tests (--tests)
        // - Apply automatic fixes (--fix)

        // Let's verify what ahma_mcp clippy support looks like within cargo.json
        let workspace_root = get_workspace_path("");
        let cargo_config_path = workspace_root.join("ahma_core/examples/configs/cargo.json");
        let cargo_config = std::fs::read_to_string(&cargo_config_path)
            .expect("Failed to read cargo.json from examples/configs");

        let cargo_json: serde_json::Value =
            serde_json::from_str(&cargo_config).expect("cargo.json should be valid JSON");

        // Find clippy subcommand inside cargo tool
        let cargo_subcommands = cargo_json["subcommand"]
            .as_array()
            .expect("cargo.json should expose subcommands");

        let clippy_cmd = cargo_subcommands
            .iter()
            .find(|cmd| cmd["name"].as_str() == Some("clippy"))
            .expect("cargo.json must include clippy subcommand");

        let options = clippy_cmd["options"]
            .as_array()
            .expect("Clippy subcommand should have options");

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

        // Ensure availability metadata is present for proactive startup checks
        let availability = clippy_cmd["availability_check"]
            .as_object()
            .expect("clippy should define availability_check");
        let command = availability
            .get("command")
            .and_then(|value| value.as_str())
            .expect("availability_check.command should exist");
        assert_eq!(
            command, "clippy-driver",
            "clippy probe should invoke clippy-driver directly to avoid cargo locking"
        );

        let args = availability
            .get("args")
            .and_then(|value| value.as_array())
            .expect("availability_check.args should exist for clippy");
        let expected_args = vec!["--version"];
        let actual_args: Vec<_> = args
            .iter()
            .map(|value| value.as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            actual_args, expected_args,
            "clippy probe should use --version"
        );

        assert!(
            availability
                .get("skip_subcommand_args")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            "clippy availability check should skip derived subcommand args"
        );

        let install_hint = clippy_cmd["install_instructions"]
            .as_str()
            .expect("clippy should include install_instructions");
        assert!(
            install_hint.contains("rustup component add clippy"),
            "clippy install guidance should point to rustup component add clippy"
        );
    }

    #[test]
    fn test_nextest_should_support_run_subcommand() {
        // TDD: nextest needs "run" subcommand to work properly

        let workspace_root = get_workspace_path("");
        let cargo_config_path = workspace_root.join("ahma_core/examples/configs/cargo.json");
        let cargo_config = std::fs::read_to_string(&cargo_config_path)
            .expect("Failed to read cargo.json from examples/configs");

        let cargo_json: serde_json::Value =
            serde_json::from_str(&cargo_config).expect("cargo.json should be valid JSON");

        let cargo_subcommands = cargo_json["subcommand"]
            .as_array()
            .expect("cargo.json should expose subcommands");

        let nextest_cmd = cargo_subcommands
            .iter()
            .find(|cmd| cmd["name"].as_str() == Some("nextest"))
            .expect("cargo.json must include nextest subcommand");

        let nested_subcommands = nextest_cmd["subcommand"]
            .as_array()
            .expect("cargo nextest should expose nested subcommands");

        assert!(
            nested_subcommands
                .iter()
                .any(|cmd| cmd["name"].as_str() == Some("run")),
            "nextest subcommand must include run"
        );

        println!(
            "Current nextest nested subcommands: {}",
            serde_json::to_string_pretty(nested_subcommands).unwrap()
        );
    }

    #[test]
    fn test_nextest_run_has_availability_and_install_guidance() {
        let workspace_root = get_workspace_path("");
        let cargo_config_path = workspace_root.join("ahma_core/examples/configs/cargo.json");
        let cargo_config = std::fs::read_to_string(&cargo_config_path)
            .expect("Failed to read cargo.json from examples/configs");

        let cargo_json: serde_json::Value =
            serde_json::from_str(&cargo_config).expect("cargo.json should be valid JSON");

        let cargo_subcommands = cargo_json["subcommand"]
            .as_array()
            .expect("cargo.json should expose subcommands");

        let nextest_cmd = cargo_subcommands
            .iter()
            .find(|cmd| cmd["name"].as_str() == Some("nextest"))
            .expect("cargo.json must include nextest subcommand");

        let nextest_install_hint = nextest_cmd["install_instructions"]
            .as_str()
            .expect("nextest should provide install instructions");
        assert!(
            nextest_install_hint.contains("cargo install cargo-nextest"),
            "nextest install guidance should reference cargo install cargo-nextest"
        );

        let nested_subcommands = nextest_cmd["subcommand"]
            .as_array()
            .expect("cargo nextest should expose nested subcommands");

        let run_cmd = nested_subcommands
            .iter()
            .find(|cmd| cmd["name"].as_str() == Some("run"))
            .expect("nextest run subcommand must exist");

        let run_availability = run_cmd["availability_check"]
            .as_object()
            .expect("nextest run should define availability_check");

        let run_command = run_availability
            .get("command")
            .and_then(|value| value.as_str())
            .expect("nextest run availability command should exist");
        assert_eq!(
            run_command, "cargo",
            "nextest run probe should invoke cargo directly"
        );

        let run_args = run_availability
            .get("args")
            .and_then(|value| value.as_array())
            .expect("nextest run availability args should exist");
        let expected_args = vec!["nextest", "--version"];
        let actual_args: Vec<_> = run_args
            .iter()
            .map(|value| value.as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            actual_args, expected_args,
            "nextest run probe should check run --version"
        );

        assert!(
            run_availability
                .get("skip_subcommand_args")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            "nextest run availability check should skip derived subcommand args"
        );

        let run_install_hint = run_cmd["install_instructions"]
            .as_str()
            .expect("nextest run should include install instructions");
        assert!(
            run_install_hint.contains("cargo install cargo-nextest"),
            "nextest run install guidance should reuse nextest instructions"
        );
    }

    #[test]
    fn test_verify_tools_directory_toml_files() {
        // TDD: Check if there are any .toml files in tools directory that need validation

        let tools_dir = get_tools_dir();
        assert!(tools_dir.exists(), "Tools directory should exist");

        let mut toml_files = Vec::new();
        let mut all_files = Vec::new();

        for entry in std::fs::read_dir(&tools_dir).expect("Failed to read tools directory") {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.is_file() {
                all_files.push(path.clone());

                if path.extension().is_some_and(|ext| ext == "toml") {
                    toml_files.push(path);
                }
            }
        }

        println!("All files in .ahma/: {:?}", all_files);
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
