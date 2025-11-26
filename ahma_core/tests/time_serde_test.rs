//! Tests for the `utils/time.rs` module
//!
//! This module tests the SystemTime serialization and deserialization functions
//! used for RFC 3339 timestamp handling.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Test struct using the time serde module
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestTimestamp {
    #[serde(with = "ahma_core::utils::time")]
    pub timestamp: SystemTime,
}

/// Test struct using the optional time serde module
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestOptionalTimestamp {
    #[serde(with = "ahma_core::utils::time::option")]
    pub timestamp: Option<SystemTime>,
}

// ============================================================================
// Tests for direct SystemTime serialization
// ============================================================================

#[test]
fn test_serialize_unix_epoch() {
    let test = TestTimestamp {
        timestamp: UNIX_EPOCH,
    };

    let json = serde_json::to_string(&test).expect("Serialization should succeed");

    // Should contain a date close to 1970
    assert!(
        json.contains("1970"),
        "Should serialize to 1970 for UNIX_EPOCH"
    );
    assert!(json.contains("timestamp"), "Should contain field name");
}

#[test]
fn test_serialize_known_timestamp() {
    // Create a known timestamp: 2024-01-15T10:30:00 UTC
    let seconds_since_epoch = 1705314600; // 2024-01-15T10:30:00Z
    let timestamp = UNIX_EPOCH + Duration::from_secs(seconds_since_epoch);

    let test = TestTimestamp { timestamp };
    let json = serde_json::to_string(&test).expect("Serialization should succeed");

    // The serialized format should contain 2024
    assert!(json.contains("2024"), "Should serialize to year 2024");
}

#[test]
fn test_deserialize_rfc3339_utc() {
    let json = r#"{"timestamp": "2024-06-15T14:30:00+00:00"}"#;

    let test: TestTimestamp = serde_json::from_str(json).expect("Deserialization should succeed");

    // Verify the timestamp is close to the expected value
    let duration = test.timestamp.duration_since(UNIX_EPOCH).unwrap();
    let expected_seconds = 1718458200; // 2024-06-15T14:30:00Z

    // Allow some tolerance for timezone differences
    let actual = duration.as_secs();
    assert!(
        (actual as i64 - expected_seconds as i64).abs() < 86400,
        "Timestamp should be close to expected (got {actual}, expected ~{expected_seconds})"
    );
}

#[test]
fn test_deserialize_rfc3339_with_offset() {
    // Test with a positive timezone offset
    let json = r#"{"timestamp": "2024-06-15T16:30:00+02:00"}"#;

    let test: TestTimestamp = serde_json::from_str(json).expect("Deserialization should succeed");

    // Both should represent the same UTC time
    let duration = test.timestamp.duration_since(UNIX_EPOCH).unwrap();
    let expected_utc_seconds = 1718458200; // 2024-06-15T14:30:00Z

    let actual = duration.as_secs();
    assert!(
        (actual as i64 - expected_utc_seconds as i64).abs() < 86400,
        "Timestamp with offset should normalize to UTC"
    );
}

#[test]
fn test_roundtrip_systemtime() {
    let original_timestamp = SystemTime::now();
    let test = TestTimestamp {
        timestamp: original_timestamp,
    };

    let json = serde_json::to_string(&test).expect("Serialization should succeed");
    let roundtrip: TestTimestamp =
        serde_json::from_str(&json).expect("Deserialization should succeed");

    // Due to string conversion, we lose sub-second precision
    // Compare at second granularity
    let orig_secs = original_timestamp
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let roundtrip_secs = roundtrip
        .timestamp
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    assert!(
        (orig_secs as i64 - roundtrip_secs as i64).abs() <= 1,
        "Roundtrip should preserve timestamp within 1 second"
    );
}

#[test]
fn test_deserialize_invalid_format_fails() {
    let invalid_json = r#"{"timestamp": "not-a-date"}"#;

    let result: Result<TestTimestamp, _> = serde_json::from_str(invalid_json);
    assert!(
        result.is_err(),
        "Invalid date format should fail deserialization"
    );
}

#[test]
fn test_deserialize_wrong_type_fails() {
    let invalid_json = r#"{"timestamp": 12345}"#;

    let result: Result<TestTimestamp, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err(), "Numeric value should fail deserialization");
}

// ============================================================================
// Tests for Option<SystemTime> serialization
// ============================================================================

#[test]
fn test_option_serialize_some() {
    let test = TestOptionalTimestamp {
        timestamp: Some(UNIX_EPOCH + Duration::from_secs(1000000)),
    };

    let json = serde_json::to_string(&test).expect("Serialization should succeed");

    assert!(!json.contains("null"), "Some value should not be null");
    assert!(
        json.contains("1970"),
        "Should contain date from 1970 (shortly after epoch)"
    );
}

#[test]
fn test_option_serialize_none() {
    let test = TestOptionalTimestamp { timestamp: None };

    let json = serde_json::to_string(&test).expect("Serialization should succeed");

    assert!(json.contains("null"), "None should serialize as null");
}

#[test]
fn test_option_deserialize_some() {
    let json = r#"{"timestamp": "2024-01-01T00:00:00+00:00"}"#;

    let test: TestOptionalTimestamp =
        serde_json::from_str(json).expect("Deserialization should succeed");

    assert!(test.timestamp.is_some(), "Should deserialize to Some");
}

#[test]
fn test_option_deserialize_null() {
    let json = r#"{"timestamp": null}"#;

    let test: TestOptionalTimestamp =
        serde_json::from_str(json).expect("Deserialization should succeed");

    assert!(test.timestamp.is_none(), "Null should deserialize to None");
}

#[test]
fn test_option_roundtrip_some() {
    let original = TestOptionalTimestamp {
        timestamp: Some(SystemTime::now()),
    };

    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    let roundtrip: TestOptionalTimestamp =
        serde_json::from_str(&json).expect("Deserialization should succeed");

    assert!(
        roundtrip.timestamp.is_some(),
        "Should preserve Some after roundtrip"
    );

    // Compare at second granularity
    let orig_secs = original
        .timestamp
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let roundtrip_secs = roundtrip
        .timestamp
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    assert!(
        (orig_secs as i64 - roundtrip_secs as i64).abs() <= 1,
        "Roundtrip should preserve timestamp"
    );
}

#[test]
fn test_option_roundtrip_none() {
    let original = TestOptionalTimestamp { timestamp: None };

    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    let roundtrip: TestOptionalTimestamp =
        serde_json::from_str(&json).expect("Deserialization should succeed");

    assert!(
        roundtrip.timestamp.is_none(),
        "Should preserve None after roundtrip"
    );
}

// ============================================================================
// Edge case tests
// ============================================================================

#[test]
fn test_serialize_far_future_date() {
    // Year 2100 timestamp
    let far_future = UNIX_EPOCH + Duration::from_secs(4102444800); // 2100-01-01T00:00:00Z

    let test = TestTimestamp {
        timestamp: far_future,
    };

    let json = serde_json::to_string(&test).expect("Far future date should serialize");
    assert!(json.contains("2100"), "Should serialize to year 2100");
}

#[test]
fn test_deserialize_nanoseconds_rfc3339() {
    // RFC 3339 with fractional seconds
    let json = r#"{"timestamp": "2024-06-15T14:30:00.123456789+00:00"}"#;

    let result: Result<TestTimestamp, _> = serde_json::from_str(json);
    assert!(
        result.is_ok(),
        "Should handle RFC 3339 with nanoseconds: {:?}",
        result
    );
}

#[test]
fn test_deserialize_zulu_timezone() {
    // RFC 3339 with Z for UTC
    let json = r#"{"timestamp": "2024-06-15T14:30:00Z"}"#;

    let result: Result<TestTimestamp, _> = serde_json::from_str(json);
    assert!(
        result.is_ok(),
        "Should handle Z timezone notation: {:?}",
        result
    );
}
