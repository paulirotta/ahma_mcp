use ahma_core::schema_validation::{MtdfValidator, ValidationErrorType};
use std::path::PathBuf;
use serde_json::json;

fn main() {
    let validator = MtdfValidator::new();
    
    let contradictory_config = json!({
        "name": "contradictory_test",
        "description": "Test contradictory descriptions",
        "command": "contradict",
        "subcommand": [
            {
                "name": "sync_with_async_desc",
                "description": "This command returns an operation_id and sends notifications asynchronously",
                "synchronous": true
            }
        ]
    }).to_string();

    let result = validator.validate_tool_config(&PathBuf::from("contradictory.json"), &contradictory_config);
    
    match result {
        Ok(_) => println!("Validation passed (unexpected)"),
        Err(errors) => {
            println!("Found {} errors:", errors.len());
            for (i, error) in errors.iter().enumerate() {
                println!("{}. Type: {:?}, Field: {}, Message: {}", 
                    i+1, error.error_type, error.field_path, error.message);
            }
            
            let has_logical_inconsistency = errors.iter().any(|e| 
                e.error_type == ValidationErrorType::LogicalInconsistency
            );
            println!("Has LogicalInconsistency: {}", has_logical_inconsistency);
            
            let has_mentions = errors.iter().any(|e| e.message.contains("mentions"));
            println!("Has 'mentions' in message: {}", has_mentions);
            
            let has_async_behavior = errors.iter().any(|e| e.message.contains("async behavior"));
            println!("Has 'async behavior' in message: {}", has_async_behavior);
        }
    }
}
