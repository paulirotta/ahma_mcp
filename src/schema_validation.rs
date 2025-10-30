//! JSON Schema validation system for tool configurations
//!
//! This module provides comprehensive validation for tool JSON configurations,
//! with detailed error messages and helpful suggestions for developers.

use crate::config::ToolConfig;
use serde_json::Value;
use std::{collections::HashSet, path::Path};

#[derive(Debug, Clone)]
pub struct SchemaValidationError {
    pub field_path: String,
    pub error_type: ValidationErrorType,
    pub message: String,
    pub suggestion: Option<String>,
}

impl std::fmt::Display for SchemaValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.field_path, self.message)?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n   💡 Suggestion: {}", suggestion)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationErrorType {
    MissingRequired,
    InvalidType,
    InvalidValue,
    UnknownField,
    ConstraintViolation,
    LogicalInconsistency,
}

pub struct MtdfValidator {
    pub strict_mode: bool,
    pub allow_unknown_fields: bool,
}

impl Default for MtdfValidator {
    fn default() -> Self {
        Self {
            strict_mode: true,
            allow_unknown_fields: false,
        }
    }
}

impl MtdfValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn with_unknown_fields_allowed(mut self, allow: bool) -> Self {
        self.allow_unknown_fields = allow;
        self
    }

    /// Validate a complete tool configuration JSON file
    pub fn validate_tool_config(
        &self,
        _json_path: &Path,
        content: &str,
    ) -> Result<ToolConfig, Vec<SchemaValidationError>> {
        let value: Value = match serde_json::from_str(content) {
            Ok(v) => v,
            Err(e) => {
                return Err(vec![SchemaValidationError {
                    field_path: "root".to_string(),
                    error_type: ValidationErrorType::InvalidValue,
                    message: format!("Invalid JSON syntax: {}", e),
                    suggestion: Some("Ensure the JSON file has proper syntax. Common issues: missing commas, trailing commas, unescaped quotes, or mismatched brackets.".to_string()),
                }]);
            }
        };

        let mut errors = Vec::new();

        // Validate the root object structure
        if !value.is_object() {
            errors.push(SchemaValidationError {
                field_path: "root".to_string(),
                error_type: ValidationErrorType::InvalidType,
                message: "Root must be a JSON object".to_string(),
                suggestion: Some("Wrap your configuration in curly braces: { ... }".to_string()),
            });
            return Err(errors);
        }

        let obj = value.as_object().unwrap();

        // Validate required root fields
        self.validate_root_fields(obj, &mut errors);

        // Validate subcommands array
        if let Some(subcommands) = obj.get("subcommand") {
            self.validate_subcommands_array(subcommands, obj, &mut errors);
        }

        // Additional semantic validations
        self.validate_semantic_consistency(obj, &mut errors);

        if errors.is_empty() {
            match serde_json::from_str::<ToolConfig>(content) {
                Ok(config) => Ok(config),
                Err(e) => Err(vec![SchemaValidationError {
                    field_path: "deserialization".to_string(),
                    error_type: ValidationErrorType::LogicalInconsistency,
                    message: format!("Configuration passed validation but failed deserialization: {e}"),
                    suggestion: Some("This may indicate a bug in the schema validator. Please report this issue.".to_string()),
                }])
            }
        } else {
            Err(errors)
        }
    }

    fn validate_root_fields(
        &self,
        obj: &serde_json::Map<String, Value>,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        let required_fields = vec![
            ("name", "string", "Tool name (e.g., 'cargo', 'git', 'npm')"),
            (
                "description",
                "string",
                "Brief description of what this tool does",
            ),
            (
                "command",
                "string",
                "Base command to execute (e.g., 'cargo', 'git')",
            ),
        ];

        let optional_fields = vec![
            (
                "enabled",
                "boolean",
                "Whether the tool is enabled (default: true)",
            ),
            (
                "timeout_seconds",
                "number",
                "Default timeout in seconds (default: 300)",
            ),
            (
                "synchronous",
                "boolean",
                "Default synchronous behavior for all subcommands (can be overridden per subcommand)",
            ),
            ("hints", "object", "Context-aware hints for AI agents"),
            (
                "guidance_key",
                "string",
                "Key to reference shared guidance from tool_guidance.json",
            ),
            ("subcommand", "array", "Array of subcommand definitions"),
            (
                "sequence",
                "array",
                "Optional sequence of tools to execute in order (for composite tools)",
            ),
            (
                "step_delay_ms",
                "number",
                "Delay in milliseconds between sequence steps (default: SEQUENCE_STEP_DELAY_MS)",
            ),
        ];

        // Check required fields
        for (field, expected_type, description) in &required_fields {
            match obj.get(*field) {
                None => {
                    errors.push(SchemaValidationError {
                        field_path: (*field).to_string(),
                        error_type: ValidationErrorType::MissingRequired,
                        message: format!("Missing required field '{}'", field),
                        suggestion: Some(format!(
                            "Add \"{}\": <{}> - {}",
                            field, expected_type, description
                        )),
                    });
                }
                Some(value) => {
                    self.validate_field_type(field, value, expected_type, errors);
                }
            }
        }

        // Validate optional fields if present
        for (field, expected_type, _description) in &optional_fields {
            if let Some(value) = obj.get(*field) {
                self.validate_field_type(field, value, expected_type, errors);
            }
        }

        // Check for unknown fields if in strict mode
        if self.strict_mode && !self.allow_unknown_fields {
            let all_known_fields: HashSet<&str> = required_fields
                .iter()
                .chain(optional_fields.iter())
                .map(|(field, _, _)| *field)
                .collect();

            for key in obj.keys() {
                if !all_known_fields.contains(key.as_str()) {
                    errors.push(SchemaValidationError {
                        field_path: key.clone(),
                        error_type: ValidationErrorType::UnknownField,
                        message: format!("Unknown field '{}'", key),
                        suggestion: Some(format!(
                            "Known fields: {}. Check for typos or remove this field.",
                            all_known_fields
                                .iter()
                                .copied()
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    });
                }
            }
        }
    }

    fn validate_subcommands_array(
        &self,
        subcommands: &Value,
        parent_tool: &serde_json::Map<String, Value>,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        if !subcommands.is_array() {
            errors.push(SchemaValidationError {
                field_path: "subcommand".to_string(),
                error_type: ValidationErrorType::InvalidType,
                message: "subcommand must be an array".to_string(),
                suggestion: Some(
                    "Use array syntax: \"subcommand\": [{ ... }, { ... }]".to_string(),
                ),
            });
            return;
        }

        let arr = subcommands.as_array().unwrap();
        for (i, subcommand) in arr.iter().enumerate() {
            let path = format!("subcommand[{}]", i);

            if !subcommand.is_object() {
                errors.push(SchemaValidationError {
                    field_path: path,
                    error_type: ValidationErrorType::InvalidType,
                    message: "Each subcommand must be an object".to_string(),
                    suggestion: Some(
                        "Use object syntax: { \"name\": \"build\", \"description\": \"...\", ... }"
                            .to_string(),
                    ),
                });
                continue;
            }

            let obj = subcommand.as_object().unwrap();
            self.validate_subcommand_fields(obj, &path, parent_tool, errors);
        }
    }

    fn validate_subcommand_fields(
        &self,
        obj: &serde_json::Map<String, Value>,
        path: &str,
        parent_tool: &serde_json::Map<String, Value>,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        let required_fields = vec![
            (
                "name",
                "string",
                "Subcommand name (e.g., 'build', 'test', 'install')",
            ),
            (
                "description",
                "string",
                "Description including async behavior guidance",
            ),
        ];

        let optional_fields = vec![
            (
                "enabled",
                "boolean",
                "Whether this subcommand is enabled (default: true)",
            ),
            (
                "synchronous",
                "boolean",
                "Whether this subcommand runs synchronously (default: false)",
            ),
            (
                "guidance_key",
                "string",
                "Key to reference shared guidance from tool_guidance.json",
            ),
            ("options", "array", "Array of command-line options"),
            (
                "subcommand",
                "array",
                "Array of nested subcommand definitions",
            ),
        ];

        // Check required fields
        for (field, expected_type, description) in &required_fields {
            match obj.get(*field) {
                None => {
                    errors.push(SchemaValidationError {
                        field_path: format!("{}.{}", path, field),
                        error_type: ValidationErrorType::MissingRequired,
                        message: format!("Missing required field '{}' in subcommand", field),
                        suggestion: Some(format!(
                            "Add \"{}\": <{}> - {}",
                            field, expected_type, description
                        )),
                    });
                }
                Some(value) => {
                    self.validate_field_type(
                        &format!("{}.{}", path, field),
                        value,
                        expected_type,
                        errors,
                    );
                }
            }
        }

        // Validate optional fields if present
        for (field, expected_type, _description) in &optional_fields {
            if let Some(value) = obj.get(*field) {
                self.validate_field_type(
                    &format!("{}.{}", path, field),
                    value,
                    expected_type,
                    errors,
                );
            }
        }

        // Validate nested subcommands array if present
        if let Some(subcommands) = obj.get("subcommand") {
            self.validate_subcommands_array(subcommands, parent_tool, errors);
        }

        // Validate options array if present
        if let Some(options) = obj.get("options") {
            self.validate_options_array(options, &format!("{}.options", path), errors);
        }

        // Validate async behavior guidelines
        if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
            self.validate_async_behavior_guidance(desc, path, obj, parent_tool, errors);
        }

        // Check for unknown fields if in strict mode
        if self.strict_mode && !self.allow_unknown_fields {
            let all_known_fields: HashSet<&str> = required_fields
                .iter()
                .chain(optional_fields.iter())
                .map(|(field, _, _)| *field)
                .chain(["positional_args"].iter().copied()) // Add positional_args as known field
                .collect();

            for key in obj.keys() {
                if !all_known_fields.contains(key.as_str()) {
                    errors.push(SchemaValidationError {
                        field_path: format!("{}.{}", path, key),
                        error_type: ValidationErrorType::UnknownField,
                        message: format!("Unknown field '{}' in subcommand", key),
                        suggestion: Some(format!(
                            "Known fields: {}. Check for typos or remove this field.",
                            all_known_fields
                                .iter()
                                .copied()
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    });
                }
            }
        }
    }

    fn validate_options_array(
        &self,
        options: &Value,
        path: &str,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        if !options.is_array() {
            errors.push(SchemaValidationError {
                field_path: path.to_string(),
                error_type: ValidationErrorType::InvalidType,
                message: "options must be an array".to_string(),
                suggestion: Some("Use array syntax: \"options\": [{ ... }, { ... }]".to_string()),
            });
            return;
        }

        let arr = options.as_array().unwrap();
        for (i, option) in arr.iter().enumerate() {
            let option_path = format!("{}[{}]", path, i);

            if !option.is_object() {
                errors.push(SchemaValidationError {
                    field_path: option_path,
                    error_type: ValidationErrorType::InvalidType,
                    message: "Each option must be an object".to_string(),
                    suggestion: Some(
                        "Use object syntax: { \"name\": \"release\", \"type\": \"boolean\", ... }"
                            .to_string(),
                    ),
                });
                continue;
            }

            let obj = option.as_object().unwrap();
            self.validate_option_fields(obj, &option_path, errors);
        }
    }

    fn validate_option_fields(
        &self,
        obj: &serde_json::Map<String, Value>,
        path: &str,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        let required_fields = vec![
            (
                "name",
                "string",
                "Option name (e.g., 'release', 'verbose', 'target')",
            ),
            (
                "type",
                "string",
                "Data type: boolean, string, integer, or array",
            ),
        ];

        let optional_fields = vec![("description", "string", "Option description")];

        // Check required fields
        for (field, expected_type, description) in required_fields {
            match obj.get(field) {
                None => {
                    errors.push(SchemaValidationError {
                        field_path: format!("{}.{}", path, field),
                        error_type: ValidationErrorType::MissingRequired,
                        message: format!("Missing required field '{}' in option", field),
                        suggestion: Some(format!(
                            "Add \"{}\": <{}> - {}",
                            field, expected_type, description
                        )),
                    });
                }
                Some(value) => {
                    self.validate_field_type(
                        &format!("{}.{}", path, field),
                        value,
                        expected_type,
                        errors,
                    );
                }
            }
        }

        // Validate type field specifically
        if let Some(type_val) = obj.get("type").and_then(|v| v.as_str()) {
            let valid_types = ["boolean", "string", "integer", "array"];
            if !valid_types.contains(&type_val) {
                let suggestion = match type_val {
                    "bool" => Some("Use 'boolean' instead of 'bool'. CLI flags like --cached, --verbose should use type 'boolean'.".to_string()),
                    "int" => Some("Use 'integer' instead of 'int'.".to_string()),
                    "str" => Some("Use 'string' instead of 'str'.".to_string()),
                    _ => Some(format!(
                        "Valid types: {}. Use one of these.",
                        valid_types.join(", ")
                    )),
                };

                errors.push(SchemaValidationError {
                    field_path: format!("{}.type", path),
                    error_type: ValidationErrorType::InvalidValue,
                    message: format!("Invalid option type '{}'", type_val),
                    suggestion,
                });
            }
        }

        // Validate optional fields if present
        for (field, expected_type, _description) in optional_fields {
            if let Some(value) = obj.get(field) {
                self.validate_field_type(
                    &format!("{}.{}", path, field),
                    value,
                    expected_type,
                    errors,
                );
            }
        }
    }

    fn validate_async_behavior_guidance(
        &self,
        description: &str,
        path: &str,
        obj: &serde_json::Map<String, Value>,
        parent_tool: &serde_json::Map<String, Value>,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        // Determine if this subcommand should be synchronous considering inheritance
        // 1. If subcommand.synchronous is Some(value), use value
        // 2. If subcommand.synchronous is None, inherit parent_tool.synchronous
        // 3. If parent_tool.synchronous is None, default to false (async)
        let is_synchronous = obj
            .get("synchronous")
            .and_then(|v| v.as_bool())
            .or_else(|| parent_tool.get("synchronous").and_then(|v| v.as_bool()))
            .unwrap_or(false);

        // Check if guidance_key is present - if so, skip guidance validation entirely
        let has_guidance_key = obj.get("guidance_key").is_some();

        if has_guidance_key {
            // Skip all guidance validation for tools using guidance_key
            return;
        }

        if !is_synchronous {
            // For async operations, provide flexible but helpful guidance validation
            let desc_lower = description.to_lowercase();
            let mut missing_guidance = Vec::new();

            // Check for asynchronous nature indication (flexible patterns)
            let async_indicators = ["asynchronous", "async", "background", "non-blocking"];
            if !async_indicators
                .iter()
                .any(|pattern| desc_lower.contains(pattern))
            {
                missing_guidance.push("async behavior indication (e.g., 'asynchronous', 'background', 'non-blocking')");
            }

            // Check for operation ID mention (flexible patterns)
            let operation_id_patterns = ["operation_id", "operation id", "id", "started"];
            if !operation_id_patterns
                .iter()
                .any(|pattern| desc_lower.contains(pattern))
            {
                missing_guidance
                    .push("operation tracking (e.g., 'operation_id', 'returns immediately')");
            }

            // Check for notification/result delivery mention (flexible patterns)
            let notification_patterns =
                ["notification", "result", "pushed", "complete", "finished"];
            if !notification_patterns
                .iter()
                .any(|pattern| desc_lower.contains(pattern))
            {
                missing_guidance
                    .push("result delivery method (e.g., 'notification', 'pushed when complete')");
            }

            // Check for non-blocking guidance (flexible patterns)
            let non_blocking_patterns = [
                "do not wait",
                "don't wait",
                "do not await",
                "don't await",
                "continue",
                "parallel",
            ];
            if !non_blocking_patterns
                .iter()
                .any(|pattern| desc_lower.contains(pattern))
            {
                missing_guidance.push(
                    "non-blocking guidance (e.g., 'continue with other tasks', 'do not wait')",
                );
            }

            // Only report errors if multiple guidance elements are missing (be more lenient)
            if missing_guidance.len() >= 3 {
                errors.push(SchemaValidationError {
                    field_path: format!("{}.description", path),
                    error_type: ValidationErrorType::ConstraintViolation,
                    message: format!(
                        "Async subcommand description should include guidance about: {}",
                        missing_guidance.join(", ")
                    ),
                    suggestion: Some(
                        "For async operations, consider including:\n\
                        • That the tool operates asynchronously/in background\n\
                        • That an operation_id is returned immediately\n\
                        • That results are delivered via notification when complete\n\
                        • That the AI should continue with other tasks (not wait/await)\n\n\
                        Example: \"Runs in background. Returns operation_id immediately. \
                        Results pushed via notification when complete. Continue with other tasks.\""
                            .to_string(),
                    ),
                });
            }
        } else {
            // For sync operations, just check for obviously contradictory async language
            let desc_lower = description.to_lowercase();
            let problematic_async_patterns = ["asynchronous", "operation_id", "notification"];

            for pattern in problematic_async_patterns {
                if desc_lower.contains(pattern) {
                    errors.push(SchemaValidationError {
                        field_path: format!("{}.description", path),
                        error_type: ValidationErrorType::LogicalInconsistency,
                        message: format!("Synchronous subcommand mentions '{}' which suggests async behavior", pattern),
                        suggestion: Some("For synchronous operations, use simple descriptions that indicate immediate results (e.g., 'Returns results immediately', 'Quick operation')".to_string()),
                    });
                    break; // Only report one such error per subcommand
                }
            }
        }
    }

    fn validate_semantic_consistency(
        &self,
        obj: &serde_json::Map<String, Value>,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        // Validate timeout_seconds is reasonable
        if let Some(timeout) = obj.get("timeout_seconds").and_then(|v| v.as_u64()) {
            if timeout < 1 {
                errors.push(SchemaValidationError {
                    field_path: "timeout_seconds".to_string(),
                    error_type: ValidationErrorType::ConstraintViolation,
                    message: "timeout_seconds should be at least 1 second".to_string(),
                    suggestion: Some("Use a timeout of at least 1 second to allow for reasonable execution time.".to_string()),
                });
            }
            if timeout > 3600 {
                errors.push(SchemaValidationError {
                    field_path: "timeout_seconds".to_string(),
                    error_type: ValidationErrorType::ConstraintViolation,
                    message: "timeout_seconds should not exceed 3600 seconds (1 hour)".to_string(),
                    suggestion: Some("Very long timeouts can cause poor user experience. Consider breaking long operations into smaller parts.".to_string()),
                });
            }
        }

        // Validate command is not empty
        if let Some(command) = obj.get("command").and_then(|v| v.as_str()) {
            if command.trim().is_empty() {
                errors.push(SchemaValidationError {
                    field_path: "command".to_string(),
                    error_type: ValidationErrorType::ConstraintViolation,
                    message: "command cannot be empty".to_string(),
                    suggestion: Some(
                        "Specify the base command to execute (e.g., 'cargo', 'git', 'npm')."
                            .to_string(),
                    ),
                });
            }
        }

        // Validate enablement logic consistency
        let tool_enabled = obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

        if let Some(subcommands) = obj.get("subcommand").and_then(|v| v.as_array()) {
            for (index, subcommand) in subcommands.iter().enumerate() {
                if let Some(sub_obj) = subcommand.as_object() {
                    if let Some(enabled) = sub_obj.get("enabled").and_then(|v| v.as_bool()) {
                        if enabled && !tool_enabled {
                            errors.push(SchemaValidationError {
                                field_path: format!("subcommand[{}].enabled", index),
                                error_type: ValidationErrorType::LogicalInconsistency,
                                message: "Subcommand has 'enabled: true' but tool is disabled at root level".to_string(),
                                suggestion: Some("Remove 'enabled: true' from subcommand (it's the default) or enable the tool at root level".to_string()),
                            });
                        }
                    }
                }
            }
        }
    }

    fn validate_field_type(
        &self,
        field_path: &str,
        value: &Value,
        expected_type: &str,
        errors: &mut Vec<SchemaValidationError>,
    ) {
        let matches = match expected_type {
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "number" => value.is_number(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => {
                errors.push(SchemaValidationError {
                    field_path: field_path.to_string(),
                    error_type: ValidationErrorType::LogicalInconsistency,
                    message: format!("Unknown expected type '{}' in validator", expected_type),
                    suggestion: Some("This is likely a bug in the schema validator.".to_string()),
                });
                return;
            }
        };

        if !matches {
            let actual_type = match value {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            };

            errors.push(SchemaValidationError {
                field_path: field_path.to_string(),
                error_type: ValidationErrorType::InvalidType,
                message: format!("Expected {}, got {}", expected_type, actual_type),
                suggestion: Some(format!(
                    "Change the value to be of type {} or fix the field definition.",
                    expected_type
                )),
            });
        }
    }

    /// Generate a helpful error report for multiple validation errors
    pub fn format_errors(&self, errors: &[SchemaValidationError], file_path: &Path) -> String {
        let mut report = String::new();

        report.push_str(&format!(
            "JSON Schema Validation Failed for: {}\n",
            file_path.display()
        ));
        report.push_str(&format!("Found {} error(s):\n\n", errors.len()));

        for (i, error) in errors.iter().enumerate() {
            report.push_str(&format!(
                "{}. [{}] {}\n",
                i + 1,
                error.field_path,
                error.message
            ));

            if let Some(suggestion) = &error.suggestion {
                report.push_str(&format!("   💡 Suggestion: {}\n", suggestion));
            }

            report.push('\n');
        }

        // Add general guidance
        report.push_str("For more information on tool JSON schema, see the documentation:\n");
        report.push_str("- docs/tool-schema-guide.md\n");
        report.push_str("- Example tool configurations in .ahma/tools/\n\n");

        report.push_str("Common fixes:\n");
        report.push_str("- Check for typos in field names\n");
        report.push_str("- Ensure all required fields are present\n");
        report.push_str("- Verify data types match expectations\n");
        report.push_str("- For async operations, include proper guidance in descriptions\n");

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::logging::init_test_logging;
    use std::path::PathBuf;

    #[test]
    fn test_valid_tool_config() {
        init_test_logging();
        let validator = MtdfValidator::new();
        let config = r#"
        {
            "name": "test_tool",
            "description": "A test tool",
            "command": "test",
            "enabled": true,
            "timeout_seconds": 300,
            "subcommand": [
                {
                    "name": "run",
                    "description": "This tool operates asynchronously. Returns operation_id immediately. Results pushed automatically via notification when complete. Continue with other tasks.",
                    "options": [
                        {
                            "name": "verbose",
                            "type": "boolean",
                            "description": "Enable verbose output"
                        }
                    ]
                }
            ]
        }"#;

        let result = validator.validate_tool_config(&PathBuf::from("test.json"), config);
        match result {
            Ok(_) => {} // Test passes
            Err(errors) => {
                println!("Validation errors: {:#?}", errors);
                panic!("Valid config should pass validation");
            }
        }
    }

    #[test]
    fn test_missing_required_fields() {
        init_test_logging();
        let validator = MtdfValidator::new();
        let config = r#"
        {
            "description": "A test tool"
        }"#;

        let result = validator.validate_tool_config(&PathBuf::from("test.json"), config);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors.iter().any(
            |e| e.field_path == "name" && e.error_type == ValidationErrorType::MissingRequired
        ));
        assert!(
            errors.iter().any(|e| e.field_path == "command"
                && e.error_type == ValidationErrorType::MissingRequired)
        );
    }

    #[test]
    fn test_invalid_async_guidance() {
        init_test_logging();
        let validator = MtdfValidator::new();
        let config = r#"
        {
            "name": "test_tool",
            "description": "A test tool",
            "command": "test",
            "subcommand": [
                {
                    "name": "run",
                    "description": "Just runs something",
                    "options": []
                }
            ]
        }"#;

        let result = validator.validate_tool_config(&PathBuf::from("test.json"), config);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        // The new validation requires 3+ missing guidance elements to trigger an error
        assert!(
            errors
                .iter()
                .any(|e| e.error_type == ValidationErrorType::ConstraintViolation
                    && e.message.contains("guidance about:"))
        );
    }

    #[test]
    fn test_synchronous_tool_validation() {
        init_test_logging();
        let validator = MtdfValidator::new();
        let config = r#"
        {
            "name": "test_tool",
            "description": "A test tool",
            "command": "test",
            "subcommand": [
                {
                    "name": "version",
                    "description": "Show version information - returns immediately.",
                    "synchronous": true,
                    "options": []
                }
            ]
        }"#;

        let result = validator.validate_tool_config(&PathBuf::from("test.json"), config);
        match result {
            Ok(_) => {} // Test passes
            Err(errors) => {
                println!("Synchronous validation errors: {:#?}", errors);
                panic!("Valid synchronous config should pass validation");
            }
        }
    }

    #[test]
    fn test_invalid_option_type() {
        init_test_logging();
        let validator = MtdfValidator::new();
        let config = r#"
        {
            "name": "test_tool",
            "description": "A test tool",
            "command": "test",
            "subcommand": [
                {
                    "name": "run",
                    "description": "Runs asynchronously in background. Returns operation_id immediately. Results pushed via notification when complete. Continue with other tasks.",
                    "options": [
                        {
                            "name": "count",
                            "type": "invalid_type",
                            "description": "Some count"
                        }
                    ]
                }
            ]
        }"#;

        let result = validator.validate_tool_config(&PathBuf::from("test.json"), config);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.error_type == ValidationErrorType::InvalidValue
                    && e.message.contains("Invalid option type"))
        );
    }

    #[test]
    fn test_bool_hint() {
        init_test_logging();
        let validator = MtdfValidator::new();
        let config = r#"
        {
            "name": "test_tool",
            "description": "A test tool",
            "command": "test",
            "subcommand": [
                {
                    "name": "run",
                    "description": "Runs asynchronously in background. Returns operation_id immediately. Results pushed via notification when complete. Continue with other tasks.",
                    "options": [
                        {
                            "name": "verbose",
                            "type": "bool",
                            "description": "Enable verbose output"
                        }
                    ]
                }
            ]
        }"#;

        let result = validator.validate_tool_config(&PathBuf::from("test.json"), config);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        // Check that we get the specific hint for using "bool" instead of "boolean"
        assert!(errors.iter().any(|e| {
            e.error_type == ValidationErrorType::InvalidValue
                && e.message.contains("Invalid option type 'bool'")
                && e.suggestion
                    .as_ref()
                    .is_some_and(|s| s.contains("Use 'boolean' instead of 'bool'"))
        }));
    }
}
