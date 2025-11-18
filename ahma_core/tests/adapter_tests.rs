//! Consolidated adapter tests
//!
//! This module consolidates all adapter-related tests into a single test binary
//! to optimize cargo nextest test discovery time.

#[path = "adapter_test.rs"]
mod adapter_test;

#[path = "adapter_comprehensive_test.rs"]
mod adapter_comprehensive_test;

#[path = "adapter_coverage_expansion_test.rs"]
mod adapter_coverage_expansion_test;

#[path = "adapter_coverage_improvement_test.rs"]
mod adapter_coverage_improvement_test;
