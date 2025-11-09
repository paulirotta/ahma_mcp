use ahma_core::schema_validation::{MtdfValidator, ValidationErrorType};
use std::path::PathBuf;
use serde_json::json;

fn main() {
    let validator = MtdfValidator::new();
    
    let mixed_config = json!({
        "name": "mixed_test", 
        "description": "Test mixed valid/invalid subcommands",
        "command": "mixed",
        "subcommand": [
            {
                "name": "valid_async",
                "description": "Valid async subcommand - returns operation_id immediately, results pushed via notification when complete, continue with other tasks",
                "synchronous": false,
                "options": [
                    {
                        "name": "valid_option",
                        "type": "boolean",
                        "description": "Valid option"
                    }
                ]
            },
            {
                "name": "invalid_sub",
                "options": [
                    {
                        "name": "invalid_option",
                        "type": "invalid_type",
                        "description": "Invalid option"
                    }
                ]
            },
            {
                "name": "another_valid",
                "description": "Another valid subcommand - synchronous operation returns results immediately", 
                "synchronous": true
            }
        ]
    }).to_string();

    let result = validator.validate_tool_config(&PathBuf::from("mixed.json"), &mixed_config);
    
    match result {
        Ok(_) => println!("Validation passed (unexpected)"),
        Err(errors) => {
            println!("Found {} errors:", errors.len());
            for (i, error) in errors.iter().enumerate() {
                println!("{}. Type: {:?}, Field: {}, Message: {}", 
                    i+1, error.error_type, error.field_path, error.message);
            }
        }
    }
}
