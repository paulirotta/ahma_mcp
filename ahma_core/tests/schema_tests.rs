//! Consolidated schema validation tests
//!
//! This module consolidates all schema validation-related tests into a single test binary
//! to optimize cargo nextest test discovery time.

#[path = "array_schema_validation_failing_test.rs"]
mod array_schema_validation_failing_test;

#[path = "array_schema_validation_test.rs"]
mod array_schema_validation_test;

#[path = "comprehensive_schema_validation_test.rs"]
mod comprehensive_schema_validation_test;

#[path = "generate_schema_test.rs"]
mod generate_schema_test;

#[path = "schema_debug_test.rs"]
mod schema_debug_test;

#[path = "schema_items_validation_test.rs"]
mod schema_items_validation_test;

#[path = "schema_validation_comprehensive_test.rs"]
mod schema_validation_comprehensive_test;

#[path = "schema_validation_fix_test.rs"]
mod schema_validation_fix_test;

#[path = "schema_validation_test.rs"]
mod schema_validation_test;

#[path = "tool_schema_validation_test.rs"]
mod tool_schema_validation_test;
