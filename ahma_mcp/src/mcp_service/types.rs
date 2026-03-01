//! Type definitions for the MCP service.
//!
//! Contains configuration structs and enums used throughout the MCP service.

use serde::Deserialize;
use std::collections::HashMap;

use crate::config::ToolConfig;

/// Distinguishes between top-level sequence tools and subcommand sequences.
/// Used by the unified sequence execution logic to handle differences in
/// tool/config lookup and message formatting.
#[derive(Clone)]
pub enum SequenceKind<'a> {
    /// Top-level sequence: each step specifies a different tool (e.g., `test_sequence`)
    TopLevel,
    /// Subcommand sequence: all steps use the same base tool config (e.g., `cargo qualitycheck`)
    #[allow(dead_code)] // base_config reserved for future use
    Subcommand { base_config: &'a ToolConfig },
}

/// Represents the structure of the guidance JSON file.
#[derive(Deserialize, Debug, Clone)]
pub struct GuidanceConfig {
    pub guidance_blocks: HashMap<String, String>,
    #[serde(default)]
    pub templates: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub legacy_guidance: Option<LegacyGuidanceConfig>,
}

impl Default for GuidanceConfig {
    fn default() -> Self {
        let mut guidance_blocks = HashMap::new();
        guidance_blocks.insert(
            "async_behavior".to_string(),
            "**IMPORTANT:** This tool operates asynchronously\n1. **Immediate Response:** Returns id and status 'started'. This is NOT YET success\n2. **Final Result:** Result pushed automatically via MCP notification when complete\n\n**Your Instructions:**\n- DO NOT await for the final result unless you are at end of all tasks and have already updated the user with 'assume success but verify' results.\n- **DO** continue with other tasks that don't depend on this operation\n- You **MUST** process the future result notification to know if operation succeeded".to_string(),
        );
        guidance_blocks.insert(
            "sync_behavior".to_string(),
            "This tool runs synchronously and returns results immediately".to_string(),
        );
        guidance_blocks.insert(
            "coordination_tool".to_string(),
            "**WARNING:** This is a blocking coordination tool. Use ONLY for final project validation when no other productive work remains.".to_string(),
        );
        guidance_blocks.insert(
            "python_async".to_string(),
            "**IMPORTANT:** This tool operates asynchronously.\n1. **Immediate Response:** Returns id and status 'started'. NOT success.\n2. **Final Result:** Result pushed automatically via MCP notification when complete.\n\n**Your Instructions:**\n- DO NOT await for the final result.\n- **DO** continue with other tasks that don't depend on this operation.\n- You **MUST** process the future result notification to know if operation succeeded.".to_string(),
        );
        guidance_blocks.insert(
            "python_sync".to_string(),
            "This tool runs synchronously and returns results immediately.".to_string(),
        );
        guidance_blocks.insert(
            "git_operations".to_string(),
            "This tool runs synchronously and returns results immediately.".to_string(),
        );
        guidance_blocks.insert(
            "cancellation_restart_hint".to_string(),
            "An operation was cancelled. Include the cancellation reason back to the user, and suggest a tool hint to restart or check status: 1) Call 'status' with the id to confirm state; 2) If appropriate, restart the tool with the same parameters; 3) Consider 'await' only when results are actually needed.".to_string(),
        );

        let mut templates = HashMap::new();
        templates.insert(
            "working_progress".to_string(),
            serde_json::json!("While {tool} operations run, consider reviewing {suggestions}"),
        );
        templates.insert(
            "standard_hints".to_string(),
            serde_json::json!({
                "build": "Building in progress - review compilation output for warnings, plan deployment steps, or work on documentation.",
                "test": "Tests running - analyze test patterns, consider additional test cases, or review code coverage strategies.",
                "doc": "Documentation building - consider reviewing doc comments, planning API improvements, or working on examples.",
                "format": "Code formatting in progress - plan refactoring opportunities or review code patterns while waiting."
            }),
        );

        Self {
            guidance_blocks,
            templates,
            legacy_guidance: Some(LegacyGuidanceConfig::default()),
        }
    }
}

/// Legacy guidance config structure for backward compatibility
#[derive(Deserialize, Debug, Clone)]
pub struct LegacyGuidanceConfig {
    pub general_guidance: HashMap<String, String>,
    pub tool_specific_guidance: HashMap<String, HashMap<String, String>>,
}

impl Default for LegacyGuidanceConfig {
    fn default() -> Self {
        let mut general_guidance = HashMap::new();
        general_guidance.insert(
            "default".to_string(),
            "Always use ahma_mcp for supported tools.".to_string(),
        );
        general_guidance.insert(
            "completion".to_string(),
            "Task completed. Review the output and plan your next action.".to_string(),
        );
        general_guidance.insert(
            "error".to_string(),
            "An error occurred. Check the error message for details and use 'await' for pending operations if needed.".to_string(),
        );

        let mut tool_specific_guidance = HashMap::new();

        let mut cargo_build = HashMap::new();
        cargo_build.insert("start".to_string(), "Cargo build started. Use 'await' to collect results when needed, or work on other tasks.".to_string());
        cargo_build.insert(
            "completion".to_string(),
            "Build successful. Artifacts are located in the target directory.".to_string(),
        );
        cargo_build.insert(
            "error".to_string(),
            "Build failed. Review the compilation errors to identify the issue.".to_string(),
        );
        tool_specific_guidance.insert("cargo_build".to_string(), cargo_build);

        let mut cargo_test = HashMap::new();
        cargo_test.insert(
            "start".to_string(),
            "Running tests. Use 'await' to collect results when needed.".to_string(),
        );
        cargo_test.insert("completion".to_string(), "All tests passed.".to_string());
        cargo_test.insert(
            "error".to_string(),
            "Some tests failed. Review the test output to debug.".to_string(),
        );
        tool_specific_guidance.insert("cargo_test".to_string(), cargo_test);

        let mut await_guidance = HashMap::new();
        await_guidance.insert(
            "start".to_string(),
            "Waiting for operations to complete. This is a blocking call.".to_string(),
        );
        await_guidance.insert(
            "completion".to_string(),
            "Wait complete. All specified operations have finished.".to_string(),
        );
        await_guidance.insert(
            "error".to_string(),
            "Wait timed out. Some operations may still be running.".to_string(),
        );
        tool_specific_guidance.insert("await".to_string(), await_guidance);

        Self {
            general_guidance,
            tool_specific_guidance,
        }
    }
}

/// Meta-parameters that control execution environment but should not be passed as CLI args
pub const META_PARAMS: &[&str] = &["working_directory", "execution_mode", "timeout_seconds"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guidance_config_default_contains_expected_blocks() {
        let cfg = GuidanceConfig::default();
        assert!(cfg.guidance_blocks.contains_key("async_behavior"));
        assert!(cfg.guidance_blocks.contains_key("sync_behavior"));
        assert!(cfg.guidance_blocks.contains_key("coordination_tool"));
        assert!(cfg.templates.contains_key("working_progress"));
        assert!(cfg.templates.contains_key("standard_hints"));
        assert!(cfg.legacy_guidance.is_some());
    }

    #[test]
    fn test_meta_params_contains_expected_values() {
        assert!(META_PARAMS.contains(&"working_directory"));
        assert!(META_PARAMS.contains(&"execution_mode"));
        assert!(META_PARAMS.contains(&"timeout_seconds"));
        assert_eq!(META_PARAMS.len(), 3);
    }

    #[test]
    fn test_sequence_kind_toplevel() {
        let kind = SequenceKind::TopLevel;
        match kind {
            SequenceKind::TopLevel => {} // Expected
            _ => panic!("Expected TopLevel variant"),
        }
    }

    #[test]
    fn test_guidance_config_deserialize_minimal() {
        let json = r#"{"guidance_blocks": {"test": "value"}}"#;
        let config: GuidanceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.guidance_blocks.get("test"),
            Some(&"value".to_string())
        );
        assert!(config.templates.is_empty());
        assert!(config.legacy_guidance.is_none());
    }

    #[test]
    fn test_guidance_config_deserialize_full() {
        let json = r#"{
            "guidance_blocks": {"tool1": "guidance1"},
            "templates": {"tmpl1": "template_value"},
            "legacy_guidance": {
                "general_guidance": {"key1": "val1"},
                "tool_specific_guidance": {"tool1": {"key2": "val2"}}
            }
        }"#;
        let config: GuidanceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.guidance_blocks.len(), 1);
        assert_eq!(config.templates.len(), 1);
        assert!(config.legacy_guidance.is_some());
        let legacy = config.legacy_guidance.unwrap();
        assert_eq!(legacy.general_guidance.len(), 1);
        assert_eq!(legacy.tool_specific_guidance.len(), 1);
    }
}
