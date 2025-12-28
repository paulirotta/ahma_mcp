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

/// Legacy guidance config structure for backward compatibility
#[derive(Deserialize, Debug, Clone)]
pub struct LegacyGuidanceConfig {
    pub general_guidance: HashMap<String, String>,
    pub tool_specific_guidance: HashMap<String, HashMap<String, String>>,
}

/// Meta-parameters that control execution environment but should not be passed as CLI args
pub const META_PARAMS: &[&str] = &["working_directory", "execution_mode", "timeout_seconds"];

#[cfg(test)]
mod tests {
    use super::*;

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
