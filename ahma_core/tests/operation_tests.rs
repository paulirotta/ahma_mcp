//! Consolidated operation monitor tests
//! 
//! This module consolidates all operation monitor-related tests into a single test binary
//! to optimize cargo nextest test discovery time.

#[path = "common/mod.rs"]
mod common;

#[path = "operation_cancellation_test.rs"]
mod operation_cancellation_test;

#[path = "operation_id_reuse_bug_test.rs"]
mod operation_id_reuse_bug_test;

#[path = "operation_monitor_comprehensive_test.rs"]
mod operation_monitor_comprehensive_test;

#[path = "operation_monitor_stress_test.rs"]
mod operation_monitor_stress_test;

#[path = "operation_monitor_test.rs"]
mod operation_monitor_test;

#[path = "test_async_operation_response_includes_tool_hints.rs"]
mod test_async_operation_response_includes_tool_hints;
