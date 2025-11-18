//! Consolidated configuration, callback, logging, and miscellaneous tests
//!
//! This module consolidates all config, callback, logging, terminal output,
//! and other miscellaneous tests into a single test binary
//! to optimize cargo nextest test discovery time.

pub use ahma_core::test_utils as common;

#[path = "ai_callback_clarity_test.rs"]
mod ai_callback_clarity_test;

#[path = "async_callback_notification_test.rs"]
mod async_callback_notification_test;

#[path = "async_notification_debug.rs"]
mod async_notification_debug;

#[path = "async_notification_delivery_test.rs"]
mod async_notification_delivery_test;

#[path = "callback_system_test.rs"]
mod callback_system_test;

#[path = "config_loading_test.rs"]
mod config_loading_test;

#[path = "config_test.rs"]
mod config_test;

#[path = "continuous_update_test.rs"]
mod continuous_update_test;

#[path = "development_workflow_invariants_test.rs"]
mod development_workflow_invariants_test;

#[path = "endless_notification_bug_test.rs"]
mod endless_notification_bug_test;

#[path = "endless_notification_fix_test.rs"]
mod endless_notification_fix_test;

#[path = "freeform_args_test.rs"]
mod freeform_args_test;

#[path = "full_system_integration_bug_test.rs"]
mod full_system_integration_bug_test;

#[path = "graceful_shutdown_test.rs"]
mod graceful_shutdown_test;

#[path = "guard_rail_test.rs"]
mod guard_rail_test;

#[path = "logging_test.rs"]
mod logging_test;

#[path = "main_test.rs"]
mod main_test;

#[path = "multiline_argument_handling_test.rs"]
mod multiline_argument_handling_test;

#[path = "path_security_test.rs"]
mod path_security_test;

#[path = "polling_detection_test.rs"]
mod polling_detection_test;

#[path = "race_condition_bug_test.rs"]
mod race_condition_bug_test;

#[path = "realistic_endless_notification_test.rs"]
mod realistic_endless_notification_test;

#[path = "refactored_quality_check_test.rs"]
mod refactored_quality_check_test;

#[path = "simple_test_discovery.rs"]
mod simple_test_discovery;

#[path = "status_polling_anti_pattern_test.rs"]
mod status_polling_anti_pattern_test;

#[path = "terminal_output_coverage_test.rs"]
mod terminal_output_coverage_test;

#[path = "terminal_output_test.rs"]
mod terminal_output_test;

#[path = "test_rust_quality_check_failure_detection.rs"]
mod test_rust_quality_check_failure_detection;

#[path = "time_utilities_coverage_test.rs"]
mod time_utilities_coverage_test;
