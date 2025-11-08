use ahma_core::{
    schema_validation::{MtdfValidator, SchemaValidationError, ValidationErrorType},
    utils::logging::init_test_logging,
};
use std::path::Path;

#[test]
fn test_mtdf_validator_default() {
    init_test_logging();
    let validator = MtdfValidator::default();
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_new() {
    init_test_logging();
    let validator = MtdfValidator::new();
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_with_strict_mode() {
    init_test_logging();
    let validator = MtdfValidator::new().with_strict_mode(false);
    assert!(!validator.strict_mode);
    assert!(!validator.allow_unknown_fields);

    let validator = MtdfValidator::new().with_strict_mode(true);
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_with_unknown_fields_allowed() {
    init_test_logging();
    let validator = MtdfValidator::new().with_unknown_fields_allowed(true);
    assert!(validator.strict_mode);
    assert!(validator.allow_unknown_fields);

    let validator = MtdfValidator::new().with_unknown_fields_allowed(false);
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_builder_chain() {
    init_test_logging();
    let validator = MtdfValidator::new()
        .with_strict_mode(false)
        .with_unknown_fields_allowed(true);

    assert!(!validator.strict_mode);
    assert!(validator.allow_unknown_fields);
}

#[test]
fn test_schema_validation_error_display() {
    init_test_logging();
    let error = SchemaValidationError {
        field_path: "test.field".to_string(),
        error_type: ValidationErrorType::MissingRequiredField,
        message: "Field is required".to_string(),
        suggestion: Some("Add the missing field".to_string()),
    };

    let display = format!("{}", error);
    // Display format: "{error_type}: {field_path} - {message}"
    // error_type displays as "Missing required field" not "MissingRequiredField"
    assert!(display.contains("test.field"));
    assert!(display.contains("Field is required"));
    assert!(display.contains("Missing required field"));
}

#[test]
fn test_schema_validation_error_display_no_suggestion() {
    init_test_logging();
    let error = SchemaValidationError {
        field_path: "root".to_string(),
        error_type: ValidationErrorType::InvalidType,
        message: "Invalid type".to_string(),
        suggestion: None,
    };

    let display = format!("{}", error);
    // Display format: "{error_type}: {field_path} - {message}"
    // error_type displays as "Invalid type" not "InvalidType"
    assert!(display.contains("root"));
    assert!(display.contains("Invalid type"));
}

#[test]
fn test_validation_error_type_equality() {
    init_test_logging();
    assert_eq!(
        ValidationErrorType::MissingRequiredField,
        ValidationErrorType::MissingRequiredField
    );
    assert_eq!(
        ValidationErrorType::InvalidType,
        ValidationErrorType::InvalidType
    );
    assert_eq!(
        ValidationErrorType::InvalidValue,
        ValidationErrorType::InvalidValue
    );
    assert_eq!(
        ValidationErrorType::UnknownField,
        ValidationErrorType::UnknownField
    );
    assert_eq!(
        ValidationErrorType::ConstraintViolation,
        ValidationErrorType::ConstraintViolation
    );
    assert_eq!(
        ValidationErrorType::LogicalInconsistency,
        ValidationErrorType::LogicalInconsistency
    );

    assert_ne!(
        ValidationErrorType::MissingRequiredField,
        ValidationErrorType::InvalidType
    );
}

#[test]
fn test_validate_tool_config_invalid_json() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let invalid_json = r#"{ "name": "test", invalid json }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field_path, "test.json");
    assert_eq!(errors[0].error_type, ValidationErrorType::InvalidFormat);
    assert!(errors[0].message.contains("Invalid JSON"));
    assert!(errors[0].suggestion.is_none());
}

#[test]
fn test_validate_tool_config_not_object() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let invalid_json = r#"["array", "instead", "of", "object"]"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field_path, "test.json");
    assert_eq!(errors[0].error_type, ValidationErrorType::SchemaViolation);
    assert!(
        errors[0].message.contains("invalid type") || errors[0].message.contains("expected a map")
    );
    // Serde doesn't provide suggestions, only error messages
    assert!(errors[0].suggestion.is_none());
}

#[test]
fn test_validate_tool_config_missing_required_fields() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let incomplete_json = r#"{ "enabled": true }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, incomplete_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    // Serde reports all missing fields in a single SchemaViolation error
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, ValidationErrorType::SchemaViolation);
    let message = &errors[0].message;
    // Check that error mentions missing required fields
    let has_missing_name = message.contains("name") || message.contains("missing field");
    assert!(
        has_missing_name,
        "Error should mention missing fields: {}",
        message
    );
}

#[test]
fn test_validate_tool_config_invalid_field_types() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let invalid_types_json = r#"{
        "name": 123,
        "description": true,
        "command": ["array", "instead", "of", "string"],
        "enabled": "string_instead_of_bool",
        "timeout_seconds": "not_a_number"
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_types_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    // Serde reports type errors as SchemaViolation, not InvalidType
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, ValidationErrorType::SchemaViolation);
    // Check error message mentions type issues
    let message = &errors[0].message;
    let has_type_error = message.contains("invalid type") || message.contains("expected");
    assert!(
        has_type_error,
        "Error should mention type issues: {}",
        message
    );
}

#[test]
fn test_validate_tool_config_unknown_fields_strict() {
    init_test_logging();
    let validator = MtdfValidator::new()
        .with_strict_mode(true)
        .with_unknown_fields_allowed(false);

    let unknown_fields_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "unknown_field": "value",
        "another_unknown": 123
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, unknown_fields_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();

    // Schema violations from serde may report multiple unknown fields in a single error
    let has_unknown_field_errors = errors.iter().any(|e| {
        e.message.contains("unknown field")
            && (e.message.contains("unknown_field") || e.message.contains("another_unknown"))
    });

    assert!(
        has_unknown_field_errors,
        "Expected errors about unknown fields, got: {:#?}",
        errors
    );
}

#[test]
fn test_validate_tool_config_unknown_fields_allowed() {
    init_test_logging();
    let validator = MtdfValidator::new()
        .with_strict_mode(true)
        .with_unknown_fields_allowed(true);

    let unknown_fields_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "unknown_field": "value",
        "another_unknown": 123
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, unknown_fields_json);

    // Should succeed or have no unknown field errors
    if let Err(errors) = result {
        let unknown_errors: Vec<&SchemaValidationError> = errors
            .iter()
            .filter(|e| e.error_type == ValidationErrorType::UnknownField)
            .collect();
        assert_eq!(unknown_errors.len(), 0);
    }
}

#[test]
fn test_validate_tool_config_non_strict_mode() {
    init_test_logging();
    let validator = MtdfValidator::new().with_strict_mode(false);

    let json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "unknown_field": "value"
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, json);

    // Should not have unknown field errors in non-strict mode
    if let Err(errors) = result {
        let unknown_errors: Vec<&SchemaValidationError> = errors
            .iter()
            .filter(|e| e.error_type == ValidationErrorType::UnknownField)
            .collect();
        assert_eq!(unknown_errors.len(), 0);
    }
}

#[test]
fn test_validate_tool_config_valid_minimal() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let valid_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test"
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, valid_json);
    assert!(result.is_ok());

    let config = result.unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.description, "Test tool");
    assert_eq!(config.command, "test");
}

#[test]
fn test_validate_tool_config_valid_complete() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let valid_json = r#"{
        "name": "cargo",
        "description": "Rust package manager",
        "command": "cargo",
        "enabled": true,
        "timeout_seconds": 300,
        "synchronous": true,
        "hints": {
            "build": "Compiling code - review for optimization opportunities",
            "test": "Tests running - analyze test patterns for improvements",
            "custom": {
                "default": "Building in progress - consider planning next steps"
            }
        },
        "guidance_key": "cargo_guidance",
        "subcommand": [
            {
                "name": "build",
                "description": "Compile the current package"
            }
        ]
    }"#;
    let path = Path::new("cargo.json");

    let result = validator.validate_tool_config(path, valid_json);
    if let Err(errors) = &result {
        eprintln!(
            "Validation errors: {}",
            validator.format_errors(errors, path)
        );
    }
    assert!(result.is_ok());

    let config = result.unwrap();
    assert_eq!(config.name, "cargo");
    assert_eq!(config.description, "Rust package manager");
    assert_eq!(config.command, "cargo");
    assert!(config.enabled);
    assert_eq!(config.timeout_seconds, Some(300));
}

#[test]
fn test_validate_subcommands_invalid_array() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let invalid_subcommands_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": "not_an_array"
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_subcommands_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();

    // Serde reports this as a single SchemaViolation
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, ValidationErrorType::SchemaViolation);
    let message = &errors[0].message;
    let has_subcommand_error =
        message.contains("expected a sequence") || message.contains("invalid type");
    assert!(
        has_subcommand_error,
        "Expected error about subcommand type, got: {}",
        message
    );
}

#[test]
fn test_validate_subcommands_invalid_object() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let invalid_subcommands_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [
            "string_instead_of_object",
            123
        ]
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_subcommands_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    // Serde reports subcommand validation errors as single SchemaViolation
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, ValidationErrorType::SchemaViolation);
    let message = &errors[0].message;
    let has_error = message.contains("subcommand")
        || message.contains("missing field")
        || message.contains("expected");
    assert!(
        has_error,
        "Expected subcommand validation error, got: {}",
        message
    );
}

#[test]
fn test_validate_subcommands_missing_required() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let incomplete_subcommands_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [
            {
                "name": "build"
            },
            {
                "description": "Missing name"
            }
        ]
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, incomplete_subcommands_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    // Serde reports missing fields as single SchemaViolation
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, ValidationErrorType::SchemaViolation);
    let message = &errors[0].message;
    let has_missing = message.contains("missing field")
        || message.contains("name")
        || message.contains("description");
    assert!(
        has_missing,
        "Expected missing field errors, got: {}",
        message
    );
}

#[test]
fn test_format_errors() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let errors = vec![
        SchemaValidationError {
            field_path: "name".to_string(),
            error_type: ValidationErrorType::MissingRequiredField,
            message: "Missing required field 'name'".to_string(),
            suggestion: Some("Add a name field".to_string()),
        },
        SchemaValidationError {
            field_path: "command".to_string(),
            error_type: ValidationErrorType::InvalidType,
            message: "Expected string".to_string(),
            suggestion: None,
        },
    ];

    let path = Path::new("test.json");
    let formatted = validator.format_errors(&errors, path);

    assert!(formatted.contains("test.json"));
    assert!(formatted.contains("Missing required field 'name'"));
    assert!(formatted.contains("Expected string"));
    assert!(formatted.contains("💡 Suggestion: Add a name field"));
    assert!(formatted.contains("'name'"));
    assert!(formatted.contains("'command'"));
}

#[test]
fn test_validate_options_array_invalid() {
    init_test_logging();
    let validator = MtdfValidator::new();
    let invalid_options_json = r#"{
        "name": "test",
        "description": "Test tool",
        "command": "test",
        "subcommand": [
            {
                "name": "build",
                "description": "Build command",
                "options": "not_an_array"
            }
        ]
    }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_options_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();

    // Serde reports type mismatches as schema violations, not necessarily mentioning "options" in field_path
    // The error message should indicate the type problem
    let has_type_error = errors
        .iter()
        .any(|e| e.message.contains("options") || e.message.contains("expected a sequence"));

    assert!(
        has_type_error,
        "Expected error about options type, got: {:#?}",
        errors
    );
}

#[test]
fn test_clone_schema_validation_error() {
    init_test_logging();
    let error = SchemaValidationError {
        field_path: "test".to_string(),
        error_type: ValidationErrorType::MissingRequiredField,
        message: "Test error".to_string(),
        suggestion: Some("Test suggestion".to_string()),
    };

    let cloned = error.clone();
    assert_eq!(error.field_path, cloned.field_path);
    assert_eq!(error.error_type, cloned.error_type);
    assert_eq!(error.message, cloned.message);
    assert_eq!(error.suggestion, cloned.suggestion);
}

#[test]
fn test_debug_schema_validation_error() {
    init_test_logging();
    let error = SchemaValidationError {
        field_path: "debug_test".to_string(),
        error_type: ValidationErrorType::InvalidValue,
        message: "Debug message".to_string(),
        suggestion: None,
    };

    let debug_str = format!("{:?}", error);
    assert!(debug_str.contains("debug_test"));
    assert!(debug_str.contains("InvalidValue"));
    assert!(debug_str.contains("Debug message"));
}

#[test]
fn test_debug_validation_error_type() {
    init_test_logging();
    let error_type = ValidationErrorType::ConstraintViolation;
    let debug_str = format!("{:?}", error_type);
    assert!(debug_str.contains("ConstraintViolation"));
}

#[test]
fn test_clone_validation_error_type() {
    init_test_logging();
    let original = ValidationErrorType::LogicalInconsistency;
    let cloned = original.clone();
    assert_eq!(original, cloned);
}
