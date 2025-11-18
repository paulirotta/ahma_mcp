//! Consolidated shell pool tests
//!
//! This module consolidates all shell pool-related tests into a single test binary
//! to optimize cargo nextest test discovery time.

#[path = "shell_async_comprehensive_test.rs"]
mod shell_async_comprehensive_test;

#[path = "shell_pool_comprehensive_test.rs"]
mod shell_pool_comprehensive_test;

#[path = "shell_pool_error_coverage_test.rs"]
mod shell_pool_error_coverage_test;

#[path = "shell_pool_stress_test.rs"]
mod shell_pool_stress_test;

#[path = "shell_pool_test.rs"]
mod shell_pool_test;
