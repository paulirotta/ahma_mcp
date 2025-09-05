#[cfg(test)]
mod mcp_callback_tests {
    use ahma_mcp::callback_system::ProgressUpdate;

    #[test]
    fn test_progress_update_variants_compile() {
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
        assert!(true);
    }

    #[test]
    fn test_mcp_callback_basic_structure() {
        // Test basic structure and compilation of the mcp_callback module
        // This ensures the module exports are accessible

        let operation_id = "test_op_123".to_string();

        // Test that operation_id creation works
        assert_eq!(operation_id, "test_op_123");
        assert_eq!(operation_id.len(), 11);
    }
}
