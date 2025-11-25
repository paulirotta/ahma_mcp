//! Additional schema validation tests to improve code coverage.
//!
//! These tests target the specific functions and branches that are not yet
//! covered by existing tests in the schema_validation module.

use ahma_core::{
    schema_validation::{MtdfValidator, SchemaValidationError, ValidationErrorType},
    utils::logging::init_test_logging,
};
use serde_json::json;
use std::path::PathBuf;

// =============================================================================
// ValidationErrorType Display Tests
// =============================================================================

#[test]
fn test_validation_error_type_display_all_variants() {
    init_test_logging();

    // Test all ValidationErrorType variants Display implementation
    let variants = [
        (
            ValidationErrorType::MissingRequiredField,
            "Missing required field",
        ),
        (ValidationErrorType::InvalidType, "Invalid type"),
        (ValidationErrorType::InvalidFormat, "Invalid format"),
        (ValidationErrorType::UnknownField, "Unknown field"),
        (ValidationErrorType::InvalidValue, "Invalid value"),
        (ValidationErrorType::SchemaViolation, "Schema violation"),
        (
            ValidationErrorType::ConstraintViolation,
            "Constraint violation",
        ),
        (
            ValidationErrorType::LogicalInconsistency,
            "Logical inconsistency",
        ),
    ];

    for (variant, expected) in variants {
        assert_eq!(format!("{}", variant), expected);
    }
}

#[test]
fn test_validation_error_type_clone_and_partial_eq() {
    init_test_logging();

    let error_type = ValidationErrorType::MissingRequiredField;
    let cloned = error_type.clone();
    assert_eq!(error_type, cloned);

    // Ensure different variants are not equal
    assert_ne!(
        ValidationErrorType::MissingRequiredField,
        ValidationErrorType::InvalidType
    );
}

// =============================================================================
// SchemaValidationError Tests
// =============================================================================

#[test]
fn test_schema_validation_error_display() {
    init_test_logging();

    let error = SchemaValidationError {
        error_type: ValidationErrorType::MissingRequiredField,
        field_path: "config.name".to_string(),
        message: "Tool name is required".to_string(),
        suggestion: Some("Add a name field".to_string()),
    };

    let display = format!("{}", error);
    assert!(display.contains("Missing required field"));
    assert!(display.contains("config.name"));
    assert!(display.contains("Tool name is required"));
    // Note: Display doesn't include suggestion, only Debug does
}

#[test]
fn test_schema_validation_error_as_std_error() {
    init_test_logging();

    let error = SchemaValidationError {
        error_type: ValidationErrorType::InvalidFormat,
        field_path: "test.json".to_string(),
        message: "Invalid JSON".to_string(),
        suggestion: None,
    };

    // Test that it implements std::error::Error
    let std_error: &dyn std::error::Error = &error;
    assert!(std_error.to_string().contains("Invalid format"));
}

// =============================================================================
// MtdfValidator validate_tool_config Tests
// =============================================================================

#[test]
fn test_validate_tool_config_invalid_json() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let result =
        validator.validate_tool_config(&PathBuf::from("invalid.json"), "{ not valid json }");

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, ValidationErrorType::InvalidFormat);
    assert!(errors[0].message.contains("Invalid JSON"));
}

#[test]
fn test_validate_tool_config_missing_required_fields() {
    init_test_logging();

    let validator = MtdfValidator::new();

    // Empty object - missing name, description, command
    let result = validator.validate_tool_config(&PathBuf::from("empty.json"), r#"{}"#);

    // Should fail at deserialization stage since name/description/command are required by serde
    assert!(result.is_err());
}

#[test]
fn test_validate_tool_config_empty_name() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "",
        "description": "Test tool",
        "command": "test"
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("empty_name.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(
        |e| e.error_type == ValidationErrorType::MissingRequiredField
            && e.field_path == "name"
            && e.message.contains("cannot be empty")
    ));
}

#[test]
fn test_validate_tool_config_empty_command() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": ""
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("empty_command.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.error_type == ValidationErrorType::ConstraintViolation
                && e.field_path == "command"
                && e.message.contains("cannot be empty"))
    );
}

#[test]
fn test_validate_tool_config_empty_description() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "",
        "command": "test"
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("empty_desc.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(
        |e| e.error_type == ValidationErrorType::MissingRequiredField
            && e.field_path == "description"
            && e.message.contains("cannot be empty")
    ));
}

#[test]
fn test_validate_tool_config_timeout_too_large() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "timeout_seconds": 7200  // 2 hours - exceeds 3600 limit
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("big_timeout.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.error_type == ValidationErrorType::ConstraintViolation
                && e.field_path == "timeout_seconds"
                && e.message.contains("should not exceed 3600"))
    );
    // Check suggestion is present
    assert!(errors.iter().any(|e| e.suggestion.is_some()));
}

#[test]
fn test_validate_tool_config_timeout_zero() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "timeout_seconds": 0
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("zero_timeout.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.error_type == ValidationErrorType::ConstraintViolation
                && e.field_path == "timeout_seconds"
                && e.message.contains("should be at least 1"))
    );
}

#[test]
fn test_validate_tool_config_valid_timeout() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "timeout_seconds": 300
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("valid_timeout.json"), &config);
    assert!(result.is_ok());
}

// =============================================================================
// Subcommand Validation Tests
// =============================================================================

#[test]
fn test_validate_subcommand_empty_name() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "",
            "description": "Empty name subcommand"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("empty_sub_name.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.field_path.contains("subcommand")
        && e.field_path.contains("name")
        && e.message.contains("cannot be empty")));
}

#[test]
fn test_validate_subcommand_empty_description() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub1",
            "description": ""
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("empty_sub_desc.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.field_path.contains("subcommand")
        && e.field_path.contains("description")
        && e.message.contains("cannot be empty")));
}

#[test]
fn test_validate_subcommand_async_keywords_in_sync_command() {
    init_test_logging();

    let validator = MtdfValidator::new();
    // force_synchronous=true but description mentions async behavior
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "build",
            "description": "Build project - async operation returns operation_id immediately"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("async_in_sync.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(
        |e| e.error_type == ValidationErrorType::LogicalInconsistency
            && e.message.contains("async behavior")
    ));
}

#[test]
fn test_validate_subcommand_force_sync_inheritance() {
    init_test_logging();

    let validator = MtdfValidator::new();
    // Tool is force_synchronous=true, subcommand inherits it
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "run",
            "description": "Run the tool synchronously"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("inherited_sync.json"), &config);
    assert!(result.is_ok());
}

#[test]
fn test_validate_subcommand_override_tool_sync() {
    init_test_logging();

    let validator = MtdfValidator::new();
    // Tool is force_synchronous=true, but subcommand overrides to false
    // Description with async keywords should be allowed
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "build",
            "description": "Build project - async operation returns operation_id immediately",
            "force_synchronous": false
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("override_sync.json"), &config);
    // Should pass because subcommand overrides tool-level force_synchronous
    assert!(result.is_ok());
}

#[test]
fn test_validate_nested_subcommands() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "parent",
            "description": "Parent subcommand",
            "subcommand": [{
                "name": "child",
                "description": "Child subcommand"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("nested.json"), &config);
    assert!(result.is_ok());
}

#[test]
fn test_validate_nested_subcommand_with_error() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "parent",
            "description": "Parent subcommand",
            "subcommand": [{
                "name": "",  // Empty name - should trigger error
                "description": "Child subcommand"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("nested_error.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    // Should find error in nested path
    assert!(
        errors
            .iter()
            .any(|e| e.field_path.contains("subcommand[0].subcommand[0].name"))
    );
}

// =============================================================================
// Option Validation Tests
// =============================================================================

#[test]
fn test_validate_option_empty_name() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub",
            "description": "Subcommand",
            "options": [{
                "name": "",
                "type": "string",
                "description": "Option with empty name"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("empty_opt_name.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.field_path.contains("options")
        && e.field_path.contains("name")
        && e.message.contains("cannot be empty")));
}

#[test]
fn test_validate_option_invalid_type() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub",
            "description": "Subcommand",
            "options": [{
                "name": "opt1",
                "type": "invalid_type",
                "description": "Option with invalid type"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("invalid_opt_type.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.error_type == ValidationErrorType::InvalidType
                && e.field_path.contains("type"))
    );
}

#[test]
fn test_validate_option_bool_instead_of_boolean() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub",
            "description": "Subcommand",
            "options": [{
                "name": "opt1",
                "type": "bool",  // Should be "boolean"
                "description": "Option"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("bool_type.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.error_type == ValidationErrorType::InvalidValue
            && e.suggestion
                .as_ref()
                .map(|s| s.contains("boolean"))
                .unwrap_or(false)
    }));
}

#[test]
fn test_validate_option_int_instead_of_integer() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub",
            "description": "Subcommand",
            "options": [{
                "name": "opt1",
                "type": "int",  // Should be "integer"
                "description": "Option"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("int_type.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.suggestion
            .as_ref()
            .map(|s| s.contains("integer"))
            .unwrap_or(false)
    }));
}

#[test]
fn test_validate_option_str_instead_of_string() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub",
            "description": "Subcommand",
            "options": [{
                "name": "opt1",
                "type": "str",  // Should be "string"
                "description": "Option"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("str_type.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| {
        e.suggestion
            .as_ref()
            .map(|s| s.contains("string"))
            .unwrap_or(false)
    }));
}

#[test]
fn test_validate_option_valid_types() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [{
            "name": "sub",
            "description": "Subcommand",
            "options": [
                {"name": "str_opt", "type": "string", "description": "String option"},
                {"name": "bool_opt", "type": "boolean", "description": "Boolean option"},
                {"name": "int_opt", "type": "integer", "description": "Integer option"},
                {"name": "arr_opt", "type": "array", "description": "Array option"}
            ]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("valid_types.json"), &config);
    assert!(result.is_ok());
}

// =============================================================================
// format_errors Tests
// =============================================================================

#[test]
fn test_format_errors_single_error() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let errors = vec![SchemaValidationError {
        error_type: ValidationErrorType::MissingRequiredField,
        field_path: "name".to_string(),
        message: "Tool name is required".to_string(),
        suggestion: None,
    }];

    let report = validator.format_errors(&errors, &PathBuf::from("test.json"));

    assert!(report.contains("test.json"));
    assert!(report.contains("1 error(s)"));
    assert!(report.contains("Missing required field"));
    assert!(report.contains("name"));
    assert!(report.contains("Tool name is required"));
    assert!(report.contains("Common fixes"));
}

#[test]
fn test_format_errors_with_suggestion() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let errors = vec![SchemaValidationError {
        error_type: ValidationErrorType::InvalidValue,
        field_path: "options[0].type".to_string(),
        message: "Invalid type 'bool'".to_string(),
        suggestion: Some("Use 'boolean' instead of 'bool'".to_string()),
    }];

    let report = validator.format_errors(&errors, &PathBuf::from("config.json"));

    assert!(report.contains("💡 Suggestion"));
    assert!(report.contains("boolean"));
}

#[test]
fn test_format_errors_multiple_errors() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let errors = vec![
        SchemaValidationError {
            error_type: ValidationErrorType::MissingRequiredField,
            field_path: "name".to_string(),
            message: "Name is required".to_string(),
            suggestion: None,
        },
        SchemaValidationError {
            error_type: ValidationErrorType::ConstraintViolation,
            field_path: "timeout_seconds".to_string(),
            message: "Timeout too large".to_string(),
            suggestion: Some("Use a smaller timeout".to_string()),
        },
        SchemaValidationError {
            error_type: ValidationErrorType::LogicalInconsistency,
            field_path: "subcommand[0].description".to_string(),
            message: "Async keywords in sync command".to_string(),
            suggestion: Some("Remove async keywords or set force_synchronous to false".to_string()),
        },
    ];

    let report = validator.format_errors(&errors, &PathBuf::from("multi_error.json"));

    assert!(report.contains("3 error(s)"));
    assert!(report.contains("1."));
    assert!(report.contains("2."));
    assert!(report.contains("3."));
}

// =============================================================================
// async keyword detection tests for various keywords
// =============================================================================

#[test]
fn test_validate_async_keyword_operation_id() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "sub",
            "description": "This returns an operation_id"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("op_id.json"), &config);
    assert!(result.is_err());
}

#[test]
fn test_validate_async_keyword_notification() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "sub",
            "description": "Results are sent via notification"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("notification.json"), &config);
    assert!(result.is_err());
}

#[test]
fn test_validate_async_keyword_background() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "sub",
            "description": "Runs in the background"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("background.json"), &config);
    assert!(result.is_err());
}

#[test]
fn test_validate_async_keyword_asynchronously() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "force_synchronous": true,
        "subcommand": [{
            "name": "sub",
            "description": "Executes asynchronously"
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("asynchronously.json"), &config);
    assert!(result.is_err());
}

// =============================================================================
// Edge cases and complete config validation
// =============================================================================

#[test]
fn test_validate_complete_valid_config() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "complete_tool",
        "description": "A complete tool configuration for testing",
        "command": "mytool",
        "enabled": true,
        "timeout_seconds": 600,
        "force_synchronous": true,
        "subcommand": [
            {
                "name": "build",
                "description": "Build the project",
                "enabled": true,
                "options": [
                    {"name": "release", "type": "boolean", "description": "Build in release mode"},
                    {"name": "target", "type": "string", "description": "Target platform"},
                    {"name": "jobs", "type": "integer", "description": "Number of parallel jobs"},
                    {"name": "features", "type": "array", "description": "Features to enable"}
                ]
            },
            {
                "name": "test",
                "description": "Run tests",
                "enabled": true,
                "subcommand": [
                    {
                        "name": "unit",
                        "description": "Run unit tests"
                    },
                    {
                        "name": "integration",
                        "description": "Run integration tests"
                    }
                ]
            }
        ]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("complete.json"), &config);
    assert!(
        result.is_ok(),
        "Complete valid config should pass: {:?}",
        result
    );

    // Verify the config is properly parsed
    let parsed_config = result.unwrap();
    assert_eq!(parsed_config.name, "complete_tool");
    assert_eq!(parsed_config.timeout_seconds, Some(600));
    assert_eq!(parsed_config.force_synchronous, Some(true));
}

#[test]
fn test_validate_config_with_multiple_errors() {
    init_test_logging();

    let validator = MtdfValidator::new();
    let config = json!({
        "name": "",  // Error: empty name
        "description": "",  // Error: empty description
        "command": "",  // Error: empty command
        "timeout_seconds": 0,  // Error: zero timeout
        "force_synchronous": true,
        "subcommand": [{
            "name": "",  // Error: empty subcommand name
            "description": "async operation returns operation_id",  // Error: async keywords
            "options": [{
                "name": "",  // Error: empty option name
                "type": "bool",  // Error: invalid type
                "description": "Option"
            }]
        }]
    })
    .to_string();

    let result = validator.validate_tool_config(&PathBuf::from("multi_errors.json"), &config);
    assert!(result.is_err());
    let errors = result.unwrap_err();

    // Should have multiple errors
    assert!(
        errors.len() >= 5,
        "Expected at least 5 errors, got {}",
        errors.len()
    );
}
