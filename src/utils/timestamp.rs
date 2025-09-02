//! # Timestamp and Duration Utilities
//!
//! This module provides a collection of helper functions for working with time,
//! specifically for formatting timestamps and calculating durations. It uses the
//! `chrono` crate for handling time zones and formatting, and the standard library's
//! `std::time` for monotonic time measurements.
//!
//! ## Core Functions
//!
//! - **`format_current_time()`**: A convenience function that gets the current system
//!   time and formats it into a human-readable `H:MM:SS` string using the local time zone.
//!
//! - **`format_time(time: SystemTime)`**: Takes a `SystemTime` object and formats it
//!   into the `H:MM:SS` string format. This is the underlying logic used by
//!   `format_current_time`.
//!
//! - **`duration_as_rounded_seconds(start: Instant, end: Instant)`**: Calculates the
//!   duration between two `Instant`s and returns the result as a whole number of
//!   seconds, effectively rounding down. `Instant` is used here because it's a
//!   monotonic clock, making it ideal for measuring elapsed time without being
//!   affected by system time changes.
//!
//! - **`duration_since_as_rounded_seconds(start: Instant)`**: A helper that calculates
//!   the duration from a given `start` `Instant` to the current moment (`Instant::now()`),
//!   returning the result in whole seconds.
//!
//! ## Usage
//!
//! These utilities are used throughout the application for tasks such as:
//! - Adding timestamps to log messages.
//! - Calculating the total execution time of an operation for reporting.
//! - Displaying the current time in user-facing messages.

use chrono::{DateTime, Local};
use std::time::{Instant, SystemTime};

/// Format the current time as H:MM:SS (24-hour format) in local time
pub fn format_current_time() -> String {
    format_time(SystemTime::now())
}

/// Format a SystemTime as H:MM:SS (24-hour format) in local time
pub fn format_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    // Use platform-compatible formatting - this will work on all systems
    let formatted = datetime.format("%H:%M:%S").to_string();
    // Remove leading zero from hour if present
    if formatted.starts_with('0') && formatted.len() > 1 && formatted.chars().nth(1) != Some(':') {
        formatted[1..].to_string()
    } else {
        formatted
    }
}

/// Calculate the duration between two Instants and return as rounded seconds
pub fn duration_as_rounded_seconds(start: Instant, end: Instant) -> u64 {
    end.duration_since(start).as_secs()
}

/// Calculate the duration from an Instant to now and return as rounded seconds
pub fn duration_since_as_rounded_seconds(start: Instant) -> u64 {
    duration_as_rounded_seconds(start, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    use std::thread;

    #[test]
    fn test_format_time_basic() {
        // Test with a specific time
        let time = Local
            .with_ymd_and_hms(2023, 1, 1, 14, 30, 5)
            .unwrap()
            .into();
        let formatted = format_time(time);
        assert_eq!(formatted, "14:30:05");

        // Test single-digit hour
        let time_morning = Local.with_ymd_and_hms(2023, 1, 1, 7, 8, 9).unwrap().into();
        let formatted_morning = format_time(time_morning);
        assert_eq!(formatted_morning, "7:08:09");
    }

    #[test]
    fn test_format_time_midnight() {
        // Test with a specific time: midnight
        let time = Local.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap().into();
        let formatted = format_time(time);
        assert_eq!(formatted, "0:00:00");
    }

    #[test]
    fn test_format_time_end_of_day() {
        // Test with a specific time: end of day
        let time = Local
            .with_ymd_and_hms(2023, 1, 1, 23, 59, 59)
            .unwrap()
            .into();
        let formatted = format_time(time);
        assert_eq!(formatted, "23:59:59");
    }

    #[test]
    fn test_format_current_time_is_valid_format() {
        let formatted = format_current_time();
        // Length can be 7 (H:MM:SS) or 8 (HH:MM:SS) depending on hour
        assert!(formatted.len() >= 7 && formatted.len() <= 8);

        // Should contain colons but no decimal point
        assert!(formatted.contains(':'));
        assert!(!formatted.contains('.'));

        // Should have exactly 2 colons and no decimal points
        assert_eq!(formatted.matches(':').count(), 2);
        assert_eq!(formatted.matches('.').count(), 0);
    }

    #[test]
    fn test_duration_as_rounded_seconds() {
        let start = Instant::now();
        thread::sleep(std::time::Duration::from_millis(1100)); // Just over 1 second
        let end = Instant::now();

        let duration = duration_as_rounded_seconds(start, end);
        assert_eq!(duration, 1); // Should round down to 1 second
    }

    #[test]
    fn test_duration_since_as_rounded_seconds() {
        let start = Instant::now();
        thread::sleep(std::time::Duration::from_millis(500)); // Half a second

        let duration = duration_since_as_rounded_seconds(start);
        assert_eq!(duration, 0); // Should round down to 0 seconds
    }

    #[test]
    fn test_duration_zero() {
        let instant = Instant::now();
        let duration = duration_as_rounded_seconds(instant, instant);
        assert_eq!(duration, 0);
    }
}
