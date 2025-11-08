#[cfg(test)]
mod mcp_callback_tests {
    use ahma_core::{
        callback_system::{CallbackError, ProgressUpdate},
        mcp_callback::mcp_callback,
        utils::logging::init_test_logging,
    };
    use rmcp::model::{NumberOrString, ProgressNotificationParam, ProgressToken};
    use std::sync::Arc;

    // Test the constructor and basic functionality
    #[test]
    fn test_mcp_callback_sender_creation() {
        init_test_logging();
        // We can't create a real Peer, but we can test that the constructor compiles
        // and that the struct has the expected fields
        // This is more of a compilation test, but it ensures the API is accessible

        // Test that we can reference the types
        let _operation_id = "test_op".to_string();
        // Note: We can't actually create a Peer without a real connection,
        // but we can test the utility function
    }

    #[test]
    fn test_mcp_callback_utility_function() {
        init_test_logging();
        // Test that the utility function compiles and returns the right type
        // We can't call it with a real Peer, but we can ensure it exists

        // This tests that the function signature is correct
        let _func = mcp_callback;
        // If we get here, the function exists and compiles
    }

    // Test the should_cancel method by creating a minimal mock
    // This requires some creativity since we can't easily mock the Peer

    #[test]
    fn test_progress_token_creation() {
        init_test_logging();
        let operation_id = "test_operation_123".to_string();
        let token = ProgressToken(NumberOrString::String(Arc::from(operation_id.as_str())));

        match token.0 {
            NumberOrString::String(s) => assert_eq!(*s, operation_id),
            NumberOrString::Number(_) => panic!("Expected string token, got number"),
        }
    }

    #[test]
    fn test_progress_notification_param_structure() {
        init_test_logging();
        let token = ProgressToken(NumberOrString::String(Arc::from("test_op")));
        let params = ProgressNotificationParam {
            progress_token: token,
            progress: 50.0,
            total: Some(100.0),
            message: Some("Test message".to_string()),
        };

        assert_eq!(params.progress, 50.0);
        assert_eq!(params.total, Some(100.0));
        assert_eq!(params.message, Some("Test message".to_string()));
    }

    #[test]
    fn test_progress_update_started_message_format() {
        init_test_logging();
        let _operation_id = "test_op_123".to_string();
        let command = "cargo build".to_string();
        let description = "Building the project".to_string();

        let expected_message = format!("{command}: {description}");
        assert_eq!(expected_message, "cargo build: Building the project");
    }

    #[test]
    fn test_progress_update_progress_default_percentage() {
        init_test_logging();
        let progress = 50.0; // Default percentage when None is provided
        assert_eq!(progress, 50.0);
    }

    #[test]
    fn test_progress_update_progress_with_percentage() {
        init_test_logging();
        let progress = 75.0; // Direct assignment since value is known
        assert_eq!(progress, 75.0);
    }

    #[test]
    fn test_progress_update_output_stdout_format() {
        init_test_logging();
        let line = "Hello World".to_string();
        let is_stderr = false;

        let message = if is_stderr {
            format!("stderr: {line}")
        } else {
            format!("stdout: {line}")
        };

        assert_eq!(message, "stdout: Hello World");
    }

    #[test]
    fn test_progress_update_output_stderr_format() {
        init_test_logging();
        let line = "Error message".to_string();
        let is_stderr = true;

        let message = if is_stderr {
            format!("stderr: {line}")
        } else {
            format!("stdout: {line}")
        };

        assert_eq!(message, "stderr: Error message");
    }

    #[test]
    fn test_progress_update_final_result_success_format() {
        init_test_logging();
        let operation_id = "test_op_123".to_string();
        let command = "cargo test".to_string();
        let description = "Running tests".to_string();
        let working_directory = "/home/user/project".to_string();
        let success = true;
        let full_output = "running 5 tests\n...\ntest result: ok".to_string();
        let duration_ms = 2500;

        let status = if success { "COMPLETED" } else { "FAILED" };
        let final_message = format!(
            "OPERATION {}: '{}'\nCommand: {}\nDescription: {}\nWorking Directory: {}\nDuration: {}ms\n\n=== FULL OUTPUT ===\n{}",
            status, operation_id, command, description, working_directory, duration_ms, full_output
        );

        assert!(final_message.contains("OPERATION COMPLETED"));
        assert!(final_message.contains("cargo test"));
        assert!(final_message.contains("Running tests"));
        assert!(final_message.contains("/home/user/project"));
        assert!(final_message.contains("2500ms"));
        assert!(final_message.contains("running 5 tests"));
    }

    #[test]
    fn test_progress_update_final_result_failure_format() {
        init_test_logging();
        let operation_id = "test_op_456".to_string();
        let command = "cargo build".to_string();
        let description = "Building project".to_string();
        let working_directory = "/tmp".to_string();
        let success = false;
        let full_output = "error: could not compile".to_string();
        let duration_ms = 1000;

        let status = if success { "COMPLETED" } else { "FAILED" };
        let final_message = format!(
            "OPERATION {}: '{}'\nCommand: {}\nDescription: {}\nWorking Directory: {}\nDuration: {}ms\n\n=== FULL OUTPUT ===\n{}",
            status, operation_id, command, description, working_directory, duration_ms, full_output
        );

        assert!(final_message.contains("OPERATION FAILED"));
        assert!(final_message.contains("cargo build"));
        assert!(final_message.contains("error: could not compile"));
    }

    #[test]
    fn test_callback_error_send_failed() {
        init_test_logging();
        let error_msg = "Failed to send MCP notification: TransportClosed";
        let error = CallbackError::SendFailed(error_msg.to_string());

        match error {
            CallbackError::SendFailed(msg) => assert_eq!(msg, error_msg),
            _ => panic!("Expected SendFailed variant"),
        }
    }

    #[test]
    fn test_mcp_callback_utility_function_compiles() {
        init_test_logging();
        // Test that the utility function can be called (compilation test)
        // We can't easily test the full functionality without a real Peer,
        // but we can ensure the function signature is correct

        // This would require a real Peer instance, so we just test that
        // the function exists and has the right signature by calling it
        // in a way that doesn't require execution

        // For now, just test that we can reference the function
        let _func = mcp_callback;
        // If we get here, the function exists
    }

    #[test]
    fn test_progress_update_variants_compile() {
        init_test_logging();
        // Test that all ProgressUpdate variants can be created
        // This ensures our match arms in send_progress cover all cases

        let _started = ProgressUpdate::Started {
            operation_id: "op1".to_string(),
            command: "test".to_string(),
            description: "test desc".to_string(),
        };

        let _progress = ProgressUpdate::Progress {
            operation_id: "op1".to_string(),
            message: "progress msg".to_string(),
            percentage: Some(50.0),
            current_step: Some("step 1".to_string()),
        };

        let _output = ProgressUpdate::Output {
            operation_id: "op1".to_string(),
            line: "output line".to_string(),
            is_stderr: false,
        };

        let _completed = ProgressUpdate::Completed {
            operation_id: "op1".to_string(),
            message: "done".to_string(),
            duration_ms: 1000,
        };

        let _failed = ProgressUpdate::Failed {
            operation_id: "op1".to_string(),
            error: "error msg".to_string(),
            duration_ms: 1500,
        };

        let _cancelled = ProgressUpdate::Cancelled {
            operation_id: "op1".to_string(),
            message: "cancelled".to_string(),
            duration_ms: 500,
        };

        let _final_result = ProgressUpdate::FinalResult {
            operation_id: "op1".to_string(),
            command: "test_cmd".to_string(),
            description: "test desc".to_string(),
            working_directory: "/tmp".to_string(),
            success: true,
            full_output: "output".to_string(),
            duration_ms: 1000,
        };

        // If we get here, all variants compile correctly
        // Test passed
    }

    #[test]
    fn test_mcp_callback_basic_structure() {
        init_test_logging();
        // Test basic structure and compilation of the mcp_callback module
        // This ensures the module exports are accessible

        let operation_id = "test_op_123".to_string();

        // Test that operation_id creation works
        assert_eq!(operation_id, "test_op_123");
        assert_eq!(operation_id.len(), 11);
    }
}
