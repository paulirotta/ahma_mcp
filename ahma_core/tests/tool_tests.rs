//! Consolidated tool-specific tests
//! 
//! This module consolidates all tool-specific tests into a single test binary
//! to optimize cargo nextest test discovery time.

#[path = "common/mod.rs"]
mod common;

#[path = "advanced_await_functionality_test.rs"]
mod advanced_await_functionality_test;

#[path = "android_gradlew_test.rs"]
mod android_gradlew_test;

#[path = "await_fix_verification_test.rs"]
mod await_fix_verification_test;

#[path = "await_manual_verification_test.rs"]
mod await_manual_verification_test;

#[path = "cancel_tool_message_test.rs"]
mod cancel_tool_message_test;

#[path = "cancellation_reason_test.rs"]
mod cancellation_reason_test;

#[path = "cargo_quality_check_subcommand_test.rs"]
mod cargo_quality_check_subcommand_test;

#[path = "clippy_test.rs"]
mod clippy_test;

#[path = "comprehensive_tool_fixes_tdd.rs"]
mod comprehensive_tool_fixes_tdd;

#[path = "enhanced_await_tool_test.rs"]
mod enhanced_await_tool_test;

#[path = "file_tools_ls_bug_test.rs"]
mod file_tools_ls_bug_test;

#[path = "gh_tool_expansion_test.rs"]
mod gh_tool_expansion_test;

#[path = "gradlew_async_test.rs"]
mod gradlew_async_test;

#[path = "gradlew_interactive_test.rs"]
mod gradlew_interactive_test;

#[path = "intelligent_await_timeout_test.rs"]
mod intelligent_await_timeout_test;

#[path = "intelligent_timeout_verification_test.rs"]
mod intelligent_timeout_verification_test;

#[path = "json_tool_test.rs"]
mod json_tool_test;

#[path = "ls_tool_command_construction_test.rs"]
mod ls_tool_command_construction_test;

#[path = "sequence_tool_test.rs"]
mod sequence_tool_test;

#[path = "synchronous_inheritance_test.rs"]
mod synchronous_inheritance_test;

#[path = "synchronous_override_test.rs"]
mod synchronous_override_test;

#[path = "test_sequence_tool_blocks_when_synchronous.rs"]
mod test_sequence_tool_blocks_when_synchronous;

#[path = "timeout_parameter_test.rs"]
mod timeout_parameter_test;

#[path = "tool_availability_test.rs"]
mod tool_availability_test;

#[path = "tool_behavior_consistency_test.rs"]
mod tool_behavior_consistency_test;

#[path = "tool_loading_bug_test.rs"]
mod tool_loading_bug_test;

#[path = "tool_loading_bug_test_fixed.rs"]
mod tool_loading_bug_test_fixed;

#[path = "tool_validation_tdd_test.rs"]
mod tool_validation_tdd_test;
