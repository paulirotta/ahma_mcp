//! Test coverage for time utilities and serialization edge cases
//!
//! This test module targets untested paths in time formatting and serialization
//! to improve code coverage.

use ahma_core::utils::logging::init_test_logging;
use ahma_core::utils::time;
use ahma_core::utils::timestamp;
use chrono::{Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
struct TimeTestStruct {
    #[serde(with = "time")]
    timestamp: SystemTime,

    #[serde(with = "time::option")]
    optional_timestamp: Option<SystemTime>,
}

#[test]
fn test_time_serialization_deserialization() {
    init_test_logging();

    let now = SystemTime::now();
    let test_struct = TimeTestStruct {
        timestamp: now,
        optional_timestamp: Some(now),
    };

    // Test serialization
    let json = serde_json::to_string(&test_struct).unwrap();
    assert!(json.contains("timestamp"));
    assert!(json.contains("optional_timestamp"));

    // Test deserialization
    let deserialized: TimeTestStruct = serde_json::from_str(&json).unwrap();

    // Times should be very close (within a few milliseconds)
    let duration_diff = deserialized
        .timestamp
        .duration_since(now)
        .unwrap_or_else(|_| now.duration_since(deserialized.timestamp).unwrap());
    assert!(duration_diff < Duration::from_secs(1));

    assert!(deserialized.optional_timestamp.is_some());
}

#[test]
fn test_time_serialization_with_none() {
    init_test_logging();

    let test_struct = TimeTestStruct {
        timestamp: SystemTime::now(),
        optional_timestamp: None,
    };

    // Test serialization with None
    let json = serde_json::to_string(&test_struct).unwrap();
    assert!(json.contains("null"));

    // Test deserialization with None
    let deserialized: TimeTestStruct = serde_json::from_str(&json).unwrap();
    assert!(deserialized.optional_timestamp.is_none());
}

#[test]
fn test_time_serialization_edge_cases() {
    init_test_logging();

    // Test with UNIX epoch
    let epoch_struct = TimeTestStruct {
        timestamp: UNIX_EPOCH,
        optional_timestamp: Some(UNIX_EPOCH),
    };

    let json = serde_json::to_string(&epoch_struct).unwrap();
    let deserialized: TimeTestStruct = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.timestamp, UNIX_EPOCH);
    assert_eq!(deserialized.optional_timestamp, Some(UNIX_EPOCH));

    // Test with future time
    let future_time = SystemTime::now() + Duration::from_secs(86400); // 1 day from now
    let future_struct = TimeTestStruct {
        timestamp: future_time,
        optional_timestamp: Some(future_time),
    };

    let json = serde_json::to_string(&future_struct).unwrap();
    let deserialized: TimeTestStruct = serde_json::from_str(&json).unwrap();

    let duration_diff = deserialized
        .timestamp
        .duration_since(future_time)
        .unwrap_or_else(|_| future_time.duration_since(deserialized.timestamp).unwrap());
    assert!(duration_diff < Duration::from_secs(1));
}

#[test]
fn test_time_deserialization_invalid_formats() {
    init_test_logging();

    // Test with invalid RFC3339 format
    let invalid_json = r#"{"timestamp": "not-a-valid-date", "optional_timestamp": null}"#;
    let result: Result<TimeTestStruct, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err());

    // Test with malformed JSON
    let malformed_json = r#"{"timestamp": 12345, "optional_timestamp": null}"#;
    let result: Result<TimeTestStruct, _> = serde_json::from_str(malformed_json);
    assert!(result.is_err());

    // Test with missing required field
    let missing_field_json = r#"{"optional_timestamp": null}"#;
    let result: Result<TimeTestStruct, _> = serde_json::from_str(missing_field_json);
    assert!(result.is_err());
}

#[test]
fn test_timestamp_formatting_edge_cases() {
    init_test_logging();

    // Test midnight (0:00:00)
    let midnight = Local.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();
    let formatted = timestamp::format_time(midnight.into());
    assert_eq!(formatted, "0:00:00");

    // Test single digit hour (no leading zero)
    let single_hour = Local.with_ymd_and_hms(2023, 1, 1, 7, 8, 9).unwrap();
    let formatted = timestamp::format_time(single_hour.into());
    assert_eq!(formatted, "7:08:09");

    // Test double digit hour
    let double_hour = Local.with_ymd_and_hms(2023, 1, 1, 14, 30, 45).unwrap();
    let formatted = timestamp::format_time(double_hour.into());
    assert_eq!(formatted, "14:30:45");

    // Test end of day
    let end_of_day = Local.with_ymd_and_hms(2023, 1, 1, 23, 59, 59).unwrap();
    let formatted = timestamp::format_time(end_of_day.into());
    assert_eq!(formatted, "23:59:59");

    // Test current time formatting
    let current_formatted = timestamp::format_current_time();
    // Format is H:MM:SS or HH:MM:SS (length 7-8)
    assert!(current_formatted.len() >= 7);
    assert!(current_formatted.len() <= 8);
    assert_eq!(current_formatted.matches(':').count(), 2);

    // Verify the format looks like a time (contains digits and colons)
    assert!(current_formatted.chars().all(|c| c.is_ascii_digit() || c == ':'));
}

#[test]
fn test_duration_calculations() {
    init_test_logging();

    let start = Instant::now();

    // Test immediate duration (should be 0)
    let immediate_duration = timestamp::duration_as_rounded_seconds(start, start);
    assert_eq!(immediate_duration, 0);

    // Test duration_since with same instant
    let immediate_since = timestamp::duration_since_as_rounded_seconds(start);
    assert_eq!(immediate_since, 0);

    // Create artificial duration for testing
    let artificial_end = start + Duration::from_millis(1500); // 1.5 seconds
    let rounded_duration = timestamp::duration_as_rounded_seconds(start, artificial_end);
    assert_eq!(rounded_duration, 1); // Should round down to 1 second

    // Test with sub-second duration
    let sub_second_end = start + Duration::from_millis(800); // 0.8 seconds
    let sub_second_duration = timestamp::duration_as_rounded_seconds(start, sub_second_end);
    assert_eq!(sub_second_duration, 0); // Should round down to 0 seconds

    // Test with exactly 1 second
    let one_second_end = start + Duration::from_secs(1);
    let one_second_duration = timestamp::duration_as_rounded_seconds(start, one_second_end);
    assert_eq!(one_second_duration, 1);

    // Test with multiple seconds
    let multi_second_end = start + Duration::from_secs(5) + Duration::from_millis(750);
    let multi_second_duration = timestamp::duration_as_rounded_seconds(start, multi_second_end);
    assert_eq!(multi_second_duration, 5); // Should round down
}

#[test]
fn test_timestamp_formatting_with_timezone_independence() {
    init_test_logging();

    // Create a specific UTC time
    let utc_time = Utc.with_ymd_and_hms(2023, 6, 15, 14, 30, 45).unwrap();
    let system_time: SystemTime = utc_time.into();

    // Format it - should work regardless of local timezone
    let formatted = timestamp::format_time(system_time);

    // The exact output depends on local timezone, but format should be consistent
    assert!(formatted.contains(':'));
    assert!(formatted.len() >= 7);
    assert!(formatted.len() <= 8);

    // Should not contain fractional seconds
    assert!(!formatted.contains('.'));

    // Should not contain AM/PM (24-hour format)
    assert!(!formatted.to_lowercase().contains("am"));
    assert!(!formatted.to_lowercase().contains("pm"));
}

#[test]
fn test_format_time_hour_edge_cases() {
    init_test_logging();

    // Test all single-digit hours (0-9)
    for hour in 0..10 {
        let time = Local.with_ymd_and_hms(2023, 1, 1, hour, 30, 45).unwrap();
        let formatted = timestamp::format_time(time.into());

        if hour == 0 {
            assert_eq!(formatted, "0:30:45");
        } else {
            assert_eq!(formatted, format!("{}:30:45", hour));
        }

        // Should not have leading zero for single digit hours (except 0)
        if hour > 0 {
            assert!(!formatted.starts_with('0'));
        }
    }

    // Test all double-digit hours (10-23)
    for hour in 10..24 {
        let time = Local.with_ymd_and_hms(2023, 1, 1, hour, 30, 45).unwrap();
        let formatted = timestamp::format_time(time.into());
        assert_eq!(formatted, format!("{}:30:45", hour));
        assert_eq!(formatted.len(), 8); // Should always be 8 chars for double-digit hours
    }
}
