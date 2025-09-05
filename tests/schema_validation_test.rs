use ahma_mcp::schema_validation::{MtdfValidator, SchemaValidationError, ValidationErrorType};
use std::path::Path;

#[test]
fn test_mtdf_validator_default() {
    let validator = MtdfValidator::default();
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_new() {
    let validator = MtdfValidator::new();
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_with_strict_mode() {
    let validator = MtdfValidator::new().with_strict_mode(false);
    assert!(!validator.strict_mode);
    assert!(!validator.allow_unknown_fields);

    let validator = MtdfValidator::new().with_strict_mode(true);
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_with_unknown_fields_allowed() {
    let validator = MtdfValidator::new().with_unknown_fields_allowed(true);
    assert!(validator.strict_mode);
    assert!(validator.allow_unknown_fields);

    let validator = MtdfValidator::new().with_unknown_fields_allowed(false);
    assert!(validator.strict_mode);
    assert!(!validator.allow_unknown_fields);
}

#[test]
fn test_mtdf_validator_builder_chain() {
    let validator = MtdfValidator::new()
        .with_strict_mode(false)
        .with_unknown_fields_allowed(true);

    assert!(!validator.strict_mode);
    assert!(validator.allow_unknown_fields);
}

#[test]
fn test_schema_validation_error_display() {
    let error = SchemaValidationError {
        field_path: "test.field".to_string(),
        error_type: ValidationErrorType::MissingRequired,
        message: "Field is required".to_string(),
        suggestion: Some("Add the missing field".to_string()),
    };

    let display = format!("{}", error);
    assert!(display.contains("[test.field]"));
    assert!(display.contains("Field is required"));
    assert!(display.contains("💡 Suggestion: Add the missing field"));
}

#[test]
fn test_schema_validation_error_display_no_suggestion() {
    let error = SchemaValidationError {
        field_path: "root".to_string(),
        error_type: ValidationErrorType::InvalidType,
        message: "Invalid type".to_string(),
        suggestion: None,
    };

    let display = format!("{}", error);
    assert!(display.contains("[root]"));
    assert!(display.contains("Invalid type"));
    assert!(!display.contains("💡 Suggestion"));
}

#[test]
fn test_validation_error_type_equality() {
    assert_eq!(
        ValidationErrorType::MissingRequired,
        ValidationErrorType::MissingRequired
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
        ValidationErrorType::MissingRequired,
        ValidationErrorType::InvalidType
    );
}

#[test]
fn test_validate_tool_config_invalid_json() {
    let validator = MtdfValidator::new();
    let invalid_json = r#"{ "name": "test", invalid json }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field_path, "root");
    assert_eq!(errors[0].error_type, ValidationErrorType::InvalidValue);
    assert!(errors[0].message.contains("Invalid JSON syntax"));
    assert!(errors[0].suggestion.is_some());
}

#[test]
fn test_validate_tool_config_not_object() {
    let validator = MtdfValidator::new();
    let invalid_json = r#"["array", "instead", "of", "object"]"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, invalid_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field_path, "root");
    assert_eq!(errors[0].error_type, ValidationErrorType::InvalidType);
    assert_eq!(errors[0].message, "Root must be a JSON object");
    assert!(
        errors[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("curly braces")
    );
}

#[test]
fn test_validate_tool_config_missing_required_fields() {
    let validator = MtdfValidator::new();
    let incomplete_json = r#"{ "enabled": true }"#;
    let path = Path::new("test.json");

    let result = validator.validate_tool_config(path, incomplete_json);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.len() >= 3); // name, description, command are required

    let missing_fields: Vec<&str> = errors
        .iter()
        .filter(|e| e.error_type == ValidationErrorType::MissingRequired)
        .map(|e| e.field_path.as_str())
        .collect();

    assert!(missing_fields.contains(&"name"));
    assert!(missing_fields.contains(&"description"));
    assert!(missing_fields.contains(&"command"));
}

#[test]
fn test_validate_tool_config_invalid_field_types() {
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
    let type_errors: Vec<&SchemaValidationError> = errors
        .iter()
        .filter(|e| e.error_type == ValidationErrorType::InvalidType)
        .collect();

    assert!(type_errors.len() >= 5);
}

#[test]
fn test_validate_tool_config_unknown_fields_strict() {
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
    let unknown_errors: Vec<&SchemaValidationError> = errors
        .iter()
        .filter(|e| e.error_type == ValidationErrorType::UnknownField)
        .collect();

    assert!(unknown_errors.len() >= 2);
}

#[test]
fn test_validate_tool_config_unknown_fields_allowed() {
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
    let validator = MtdfValidator::new();
    let valid_json = r#"{
        "name": "cargo",
        "description": "Rust package manager",
        "command": "cargo",
        "enabled": true,
        "timeout_seconds": 300,
        "synchronous": true,
        "hints": {
            "default": "Building in progress - consider planning next steps",
            "build": "Compiling code - review for optimization opportunities",
            "test": "Tests running - analyze test patterns for improvements"
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
    let subcommand_errors: Vec<&SchemaValidationError> = errors
        .iter()
        .filter(|e| e.field_path == "subcommand")
        .collect();

    assert!(!subcommand_errors.is_empty());
    assert_eq!(
        subcommand_errors[0].error_type,
        ValidationErrorType::InvalidType
    );
}

#[test]
fn test_validate_subcommands_invalid_object() {
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
    let subcommand_errors: Vec<&SchemaValidationError> = errors
        .iter()
        .filter(|e| e.field_path.contains("subcommand["))
        .collect();

    assert!(subcommand_errors.len() >= 2);
}

#[test]
fn test_validate_subcommands_missing_required() {
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
    let missing_errors: Vec<&SchemaValidationError> = errors
        .iter()
        .filter(|e| e.error_type == ValidationErrorType::MissingRequired)
        .collect();

    assert!(missing_errors.len() >= 2); // Missing description in first, name in second
}

#[test]
fn test_format_errors() {
    let validator = MtdfValidator::new();
    let errors = vec![
        SchemaValidationError {
            field_path: "name".to_string(),
            error_type: ValidationErrorType::MissingRequired,
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
    assert!(formatted.contains("[name]"));
    assert!(formatted.contains("[command]"));
}

#[test]
fn test_validate_options_array_invalid() {
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
    let option_errors: Vec<&SchemaValidationError> = errors
        .iter()
        .filter(|e| e.field_path.contains("options"))
        .collect();

    assert!(!option_errors.is_empty());
}

#[test]
fn test_clone_schema_validation_error() {
    let error = SchemaValidationError {
        field_path: "test".to_string(),
        error_type: ValidationErrorType::MissingRequired,
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
    let error_type = ValidationErrorType::ConstraintViolation;
    let debug_str = format!("{:?}", error_type);
    assert!(debug_str.contains("ConstraintViolation"));
}

#[test]
fn test_clone_validation_error_type() {
    let original = ValidationErrorType::LogicalInconsistency;
    let cloned = original.clone();
    assert_eq!(original, cloned);
}
