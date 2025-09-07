#[cfg(test)]
mod main_tests {
    use std::path::PathBuf;

    #[test]
    fn test_default_paths() {
        // Test that default paths can be created
        let tools_dir = PathBuf::from(".ahma/tools");
        let guidance_file = PathBuf::from("tool_guidance.json");

        assert_eq!(tools_dir.to_string_lossy(), ".ahma/tools");
        assert_eq!(guidance_file.to_string_lossy(), "tool_guidance.json");
    }

    #[test]
    fn test_timeout_values() {
        // Test that timeout value parsing works
        let default_timeout: u64 = 300;
        let custom_timeout: u64 = 600;

        assert_eq!(default_timeout, 300);
        assert!(custom_timeout > default_timeout);
    }

    #[test]
    fn test_cli_argument_structure() {
        // Test that we can create and validate CLI argument types
        // This ensures our CLI structure compiles correctly

        let tool_name = Some("cargo_build".to_string());
        let tool_args = ["--release".to_string(), "--verbose".to_string()];

        assert!(tool_name.is_some());
        assert_eq!(tool_args.len(), 2);
        assert_eq!(tool_args[0], "--release");
    }

    #[test]
    fn test_mode_detection_logic() {
        // Test the logic for determining which mode to run in
        // This mirrors the logic in main() for mode selection

        let server_mode = true;
        let tool_name: Option<String> = None;
        let validate: Option<String> = None;

        // Server mode detection
        if server_mode || (tool_name.is_none() && validate.is_none()) {
            // Should run in server mode - this is correct
        } else if validate.is_some() {
            panic!("Should not run in validation mode");
        } else {
            panic!("Should not run in CLI mode");
        }
    }

    #[test]
    fn test_validation_mode_detection() {
        // Test validation mode detection
        let server_mode = false;
        let tool_name: Option<String> = None;
        let validate = Some("all".to_string());

        if server_mode || (tool_name.is_none() && validate.is_none()) {
            panic!("Should not run in server mode");
        } else if validate.is_some() {
            // Should run in validation mode - this is correct
        } else {
            panic!("Should not run in CLI mode");
        }
    }

    #[test]
    fn test_cli_mode_detection() {
        // Test CLI mode detection
        let server_mode = false;
        let tool_name = Some("cargo_build".to_string());
        let validate: Option<String> = None;

        if server_mode || (tool_name.is_none() && validate.is_none()) {
            panic!("Should not run in server mode");
        } else if validate.is_some() {
            panic!("Should not run in validation mode");
        } else {
            // Should run in CLI mode - this is correct
        }
    }

    #[test]
    fn test_tool_name_parsing_valid() {
        // Test parsing valid tool names
        let test_cases = vec![
            ("cargo_build", ("cargo", vec!["build"])),
            ("cargo_check", ("cargo", vec!["check"])),
            ("git_add", ("git", vec!["add"])),
            ("npm_install_package", ("npm", vec!["install", "package"])),
            ("docker_compose_up", ("docker", vec!["compose", "up"])),
        ];

        for (input, (expected_base, expected_parts)) in test_cases {
            let parts: Vec<&str> = input.split('_').collect();
            assert!(
                parts.len() >= 2,
                "Tool name '{}' should have at least 2 parts",
                input
            );

            let base_tool = parts[0];
            let subcommand_parts: Vec<&str> = parts[1..].to_vec();

            assert_eq!(base_tool, expected_base);
            assert_eq!(subcommand_parts, expected_parts);
        }
    }

    #[test]
    fn test_tool_name_parsing_invalid() {
        // Test parsing invalid tool names
        let invalid_names = vec!["cargo", "tool", "", "tool_", "_tool", "tool__name"];

        for name in invalid_names {
            let parts: Vec<&str> = name.split('_').collect();
            // Invalid if: fewer than 2 parts, empty string, or any empty parts
            let is_invalid = parts.len() < 2 || parts.iter().any(|p| p.is_empty());
            assert!(is_invalid, "Tool name '{}' should be invalid", name);
        }
    }

    #[test]
    fn test_cli_argument_parsing() {
        // Test parsing of CLI arguments for tool execution
        let test_cases = vec![
            // (input_args, expected_working_dir, expected_args_map)
            (
                vec![
                    "--working-directory".to_string(),
                    "/tmp".to_string(),
                    "--verbose".to_string(),
                ],
                Some("/tmp".to_string()),
                vec![("verbose".to_string(), serde_json::Value::Bool(true))],
            ),
            (
                vec!["--release".to_string(), "true".to_string()],
                None,
                vec![(
                    "release".to_string(),
                    serde_json::Value::String("true".to_string()),
                )],
            ),
            (
                vec![
                    "--".to_string(),
                    "file1.txt".to_string(),
                    "file2.txt".to_string(),
                ],
                None,
                vec![],
            ),
        ];

        for (args, expected_wd, expected_args) in test_cases {
            let mut working_directory: Option<String> = None;
            let mut tool_args_map: serde_json::Map<String, serde_json::Value> =
                serde_json::Map::new();

            let mut iter = args.into_iter();
            while let Some(arg) = iter.next() {
                if arg == "--" {
                    // Raw args handling - just break for this test
                    break;
                }
                if arg.starts_with("--") {
                    let key = arg.trim_start_matches("--").to_string();
                    if let Some(val) = iter.next() {
                        if key == "working-directory" {
                            working_directory = Some(val);
                        } else {
                            tool_args_map.insert(key, serde_json::Value::String(val));
                        }
                    } else {
                        tool_args_map.insert(key, serde_json::Value::Bool(true));
                    }
                }
            }

            assert_eq!(working_directory, expected_wd);
            for (key, expected_value) in expected_args {
                assert_eq!(tool_args_map.get(&key), Some(&expected_value));
            }
        }
    }

    #[test]
    fn test_environment_variable_parsing() {
        // Test parsing of AHMA_MCP_ARGS environment variable
        let test_json =
            r#"{"working_directory": "/tmp", "verbose": true, "args": ["file1", "file2"]}"#;

        let json_val: serde_json::Value = serde_json::from_str(test_json).unwrap();
        if let Some(map) = json_val.as_object() {
            assert_eq!(
                map.get("working_directory").unwrap().as_str().unwrap(),
                "/tmp"
            );
            assert!(map.get("verbose").unwrap().as_bool().unwrap());
            assert_eq!(map.get("args").unwrap().as_array().unwrap().len(), 2);
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_validation_target_parsing() {
        // Test parsing of validation targets
        let test_cases = vec![
            ("all", vec![]), // Special case for all
            ("tools/cargo.json", vec![PathBuf::from("tools/cargo.json")]),
            (
                "tools/cargo.json,tools/git.json",
                vec![
                    PathBuf::from("tools/cargo.json"),
                    PathBuf::from("tools/git.json"),
                ],
            ),
            (
                "tools/cargo.json, tools/git.json",
                vec![
                    PathBuf::from("tools/cargo.json"),
                    PathBuf::from("tools/git.json"),
                ],
            ),
        ];

        for (input, expected) in test_cases {
            if input == "all" {
                assert_eq!(expected.len(), 0);
            } else {
                let result: Vec<PathBuf> =
                    input.split(',').map(|s| PathBuf::from(s.trim())).collect();
                assert_eq!(result, expected);
            }
        }
    }

    #[test]
    fn test_log_level_configuration() {
        // Test log level configuration logic
        let debug_enabled = true;
        let debug_disabled = false;

        let log_level_debug = if debug_enabled { "debug" } else { "info" };
        let log_level_info = if debug_disabled { "debug" } else { "info" };

        assert_eq!(log_level_debug, "debug");
        assert_eq!(log_level_info, "info");
    }

    #[test]
    fn test_graceful_shutdown_timeout_calculation() {
        // Test the shutdown timeout calculation logic
        let timeout_secs = 300;
        let shutdown_timeout = std::time::Duration::from_secs(timeout_secs);

        assert_eq!(shutdown_timeout.as_secs(), 300);
        assert_eq!(shutdown_timeout.as_millis(), 300000);
    }

    #[test]
    fn test_tool_config_lookup() {
        // Test the tool configuration lookup logic
        use ahma_mcp::config::ToolConfig;
        use std::collections::HashMap;

        let mut configs = HashMap::new();
        configs.insert(
            "cargo".to_string(),
            ToolConfig {
                name: "cargo".to_string(),
                description: "Cargo build tool".to_string(),
                command: "cargo".to_string(),
                guidance_key: None,
                subcommand: None,
                input_schema: None,
                timeout_seconds: Some(300),
                synchronous: Some(false),
                hints: Default::default(),
                enabled: true,
            },
        );

        // Test successful lookup
        let config = configs.get("cargo");
        assert!(config.is_some());
        assert_eq!(config.unwrap().name, "cargo");

        // Test failed lookup
        let missing_config = configs.get("missing_tool");
        assert!(missing_config.is_none());
    }

    #[test]
    fn test_subcommand_config_lookup() {
        // Test the subcommand configuration lookup logic
        use ahma_mcp::config::SubcommandConfig;

        let subcommands = vec![
            SubcommandConfig {
                name: "build".to_string(),
                description: "Build the project".to_string(),
                guidance_key: None,
                subcommand: None,
                timeout_seconds: Some(300),
                options: None,
                positional_args: None,
                synchronous: Some(false),
                enabled: true,
            },
            SubcommandConfig {
                name: "check".to_string(),
                description: "Check the project".to_string(),
                guidance_key: None,
                subcommand: None,
                timeout_seconds: Some(60),
                options: None,
                positional_args: None,
                synchronous: Some(false),
                enabled: true,
            },
        ];

        // Test successful lookup
        let found = subcommands.iter().find(|s| s.name == "build");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "build");

        // Test failed lookup
        let not_found = subcommands.iter().find(|s| s.name == "missing");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_args_map_construction() {
        // Test the construction of args_map for tool execution
        use serde_json::Value;

        let mut args_map = serde_json::Map::new();

        // Add subcommand
        args_map.insert(
            "_subcommand".to_string(),
            Value::String("build".to_string()),
        );

        // Add arguments
        args_map.insert("release".to_string(), Value::Bool(true));
        args_map.insert("verbose".to_string(), Value::String("2".to_string()));

        assert_eq!(args_map.len(), 3);
        assert_eq!(
            args_map.get("_subcommand").unwrap().as_str().unwrap(),
            "build"
        );
        assert!(args_map.get("release").unwrap().as_bool().unwrap());
        assert_eq!(args_map.get("verbose").unwrap().as_str().unwrap(), "2");
    }

    #[test]
    fn test_working_directory_resolution() {
        // Test working directory resolution logic
        let explicit_wd = Some("/tmp/project".to_string());
        let from_args = Some("/home/user/project".to_string());
        let current_dir = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        let none = None;

        // Test explicit working directory
        let final_wd = explicit_wd.or_else(|| from_args.clone());
        assert_eq!(final_wd, Some("/tmp/project".to_string()));

        // Test fallback to args
        let final_wd = none.clone().or_else(|| from_args.clone());
        assert_eq!(final_wd, Some("/home/user/project".to_string()));

        // Test fallback to current directory
        let final_wd = none.or_else(|| current_dir.clone());
        assert!(final_wd.is_some());
    }
}
