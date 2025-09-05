#[cfg(test)]
mod mcp_service_tests {
    use ahma_mcp::config::{OptionConfig, SubcommandConfig, ToolConfig};
    use ahma_mcp::mcp_service::{GuidanceConfig, LegacyGuidanceConfig};
    use serde_json::json;

    #[test]
    fn test_guidance_config_deserialization() {
        // Test that GuidanceConfig can be properly deserialized from JSON
        let json_str = r#"{
            "guidance_blocks": {
                "async_behavior": "This tool operates asynchronously",
                "sync_behavior": "This tool runs synchronously"
            },
            "templates": {
                "default": "Default template"
            },
            "legacy_guidance": {
                "general_guidance": {
                    "wait": "Wait guidance"
                },
                "tool_specific_guidance": {
                    "cargo": {
                        "build": "Build guidance"
                    }
                }
            }
        }"#;

        let config: GuidanceConfig = serde_json::from_str(json_str).unwrap();

        assert_eq!(config.guidance_blocks.len(), 2);
        assert!(config.guidance_blocks.contains_key("async_behavior"));
        assert!(config.guidance_blocks.contains_key("sync_behavior"));
        assert_eq!(
            config.guidance_blocks["async_behavior"],
            "This tool operates asynchronously"
        );

        assert_eq!(config.templates.len(), 1);
        assert!(config.templates.contains_key("default"));

        assert!(config.legacy_guidance.is_some());
        let legacy = config.legacy_guidance.unwrap();
        assert_eq!(legacy.general_guidance.len(), 1);
        assert!(legacy.general_guidance.contains_key("wait"));
    }

    #[test]
    fn test_guidance_config_minimal() {
        // Test that GuidanceConfig works with minimal JSON (only required fields)
        let json_str = r#"{
            "guidance_blocks": {
                "test": "Test guidance"
            }
        }"#;

        let config: GuidanceConfig = serde_json::from_str(json_str).unwrap();

        assert_eq!(config.guidance_blocks.len(), 1);
        assert!(config.guidance_blocks.contains_key("test"));
        assert_eq!(config.templates.len(), 0); // Should default to empty
        assert!(config.legacy_guidance.is_none()); // Should default to None
    }

    #[test]
    fn test_legacy_guidance_config() {
        // Test LegacyGuidanceConfig structure
        let json_str = r#"{
            "general_guidance": {
                "wait": "Wait for operations",
                "status": "Check status"
            },
            "tool_specific_guidance": {
                "cargo": {
                    "build": "Build the project",
                    "test": "Run tests"
                },
                "git": {
                    "commit": "Commit changes"
                }
            }
        }"#;

        let config: LegacyGuidanceConfig = serde_json::from_str(json_str).unwrap();

        assert_eq!(config.general_guidance.len(), 2);
        assert_eq!(config.tool_specific_guidance.len(), 2);
        assert!(config.tool_specific_guidance.contains_key("cargo"));
        assert!(config.tool_specific_guidance.contains_key("git"));

        let cargo_guidance = &config.tool_specific_guidance["cargo"];
        assert_eq!(cargo_guidance.len(), 2);
        assert!(cargo_guidance.contains_key("build"));
        assert!(cargo_guidance.contains_key("test"));
    }

    #[test]
    fn test_tool_config_structure() {
        // Test that ToolConfig structures work as expected for the service
        let config = ToolConfig {
            name: "test_tool".to_string(),
            description: "Test tool description".to_string(),
            command: "test".to_string(),
            enabled: true,
            synchronous: Some(false),
            timeout_seconds: Some(300),
            guidance_key: None,
            hints: Default::default(),
            input_schema: None,
            subcommand: Some(vec![SubcommandConfig {
                name: "build".to_string(),
                description: "Build the project".to_string(),
                enabled: true,
                synchronous: Some(true),
                timeout_seconds: None,
                guidance_key: Some("sync_behavior".to_string()),
                options: Some(vec![OptionConfig {
                    name: "release".to_string(),
                    option_type: "boolean".to_string(),
                    description: "Build in release mode".to_string(),
                    required: Some(false),
                    alias: None,
                    format: None,
                    file_arg: None,
                    file_flag: None,
                }]),
                positional_args: None,
                subcommand: None,
            }]),
        };

        assert_eq!(config.name, "test_tool");
        assert_eq!(config.command, "test");
        assert!(config.enabled);
        assert_eq!(config.synchronous, Some(false));

        let subcommands = config.subcommand.as_ref().unwrap();
        assert_eq!(subcommands.len(), 1);

        let build_cmd = &subcommands[0];
        assert_eq!(build_cmd.name, "build");
        assert_eq!(build_cmd.synchronous, Some(true));
        assert_eq!(build_cmd.guidance_key, Some("sync_behavior".to_string()));

        let options = build_cmd.options.as_ref().unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "release");
        assert_eq!(options[0].option_type, "boolean");
    }

    #[test]
    fn test_environment_variable_access() {
        // Test that we can access the cargo package environment variables
        // This ensures the service can get its name and version
        let pkg_name = env!("CARGO_PKG_NAME");
        let pkg_version = env!("CARGO_PKG_VERSION");

        assert!(!pkg_name.is_empty());
        assert!(!pkg_version.is_empty());
        assert_eq!(pkg_name, "ahma_mcp");
    }

    #[test]
    fn test_json_value_parsing() {
        // Test that we can parse JSON values as expected for tool arguments
        let json_val = json!({
            "release": true,
            "features": ["serde", "tokio"],
            "jobs": 4,
            "verbose": false
        });

        if let Some(obj) = json_val.as_object() {
            assert!(obj.contains_key("release"));
            assert!(obj.contains_key("features"));
            assert!(obj.contains_key("jobs"));

            assert_eq!(obj["release"].as_bool(), Some(true));
            assert_eq!(obj["jobs"].as_i64(), Some(4));

            if let Some(features) = obj["features"].as_array() {
                assert_eq!(features.len(), 2);
                assert_eq!(features[0].as_str(), Some("serde"));
            }
        }
    }
}
