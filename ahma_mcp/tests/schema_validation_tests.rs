//! Consolidated schema_validation tests
//!
//! This module consolidates all schema_validation tests into a single test binary
//! to optimize cargo nextest test discovery time.

pub use ahma_mcp::test_utils as common;

#[path = "schema_validation/array.rs"]
mod array;

#[path = "schema_validation/array_failing.rs"]
mod array_failing;

#[path = "schema_validation/basic.rs"]
mod basic;

#[path = "schema_validation/comprehensive.rs"]
mod comprehensive;

#[path = "schema_validation/comprehensive_duplicate.rs"]
mod comprehensive_duplicate;

#[path = "schema_validation/coverage.rs"]
mod coverage;

#[path = "schema_validation/debug.rs"]
mod debug;

#[path = "schema_validation/fix.rs"]
mod fix;

#[path = "schema_validation/items.rs"]
mod items;

#[path = "schema_validation/tool.rs"]
mod tool;
