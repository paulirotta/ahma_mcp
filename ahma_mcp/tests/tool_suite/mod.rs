//! Consolidated tool_suite tests
//!
//! This module organizes tool tests into logical groups:
//! - await_tests: Tests for await tool functionality
//! - cancellation_tests: Tests for operation cancellation
//! - sequence_tests: Tests for sequence tool and synchronous behavior
//! - tool_loading_tests: Tests for tool loading and availability

// === Await Tool Tests ===
mod advanced_await_functionality_test;
mod await_fix_verification_test;
mod await_manual_verification_test;
mod enhanced_await_tool_test;
mod intelligent_await_timeout_test;
mod intelligent_timeout_verification_test;

// === Cancellation Tests ===
mod cancel_tool_message_test;
mod cancellation_reason_test;

// === Sequence and Synchronous Behavior Tests ===
mod sequence_tool_test;
mod synchronous_inheritance_test;
mod synchronous_override_test;
mod test_sequence_tool_blocks_when_synchronous;

// === Tool Loading and Availability Tests ===
mod tool_availability_test;
mod tool_loading_bug_test;
mod tool_validation_tdd_test;

// === CLI Tool Tests ===
mod android_gradlew_test;
mod cargo_quality_check_subcommand_test;
mod clippy_test;
mod gh_tool_expansion_test;
mod gradlew_async_test;
mod gradlew_interactive_test;
mod json_tool_test;

// === Tool Execution Tests ===
mod comprehensive_tool_fixes_tdd;
mod comprehensive_tool_json_coverage_test;
mod file_tools_ls_bug_test;
mod ls_tool_command_construction_test;
mod multiline_argument_handling_test;
mod timeout_parameter_test;
mod tool_behavior_consistency_test;
mod tool_execution_integration_test;
