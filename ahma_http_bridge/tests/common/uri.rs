//! File URI parsing and encoding utilities for tests.

use std::path::PathBuf;

/// Parse a file:// URI to a filesystem path.
///
/// Handles:
/// - Standard file:// URIs
/// - URL-encoded characters (%20 for space, etc.)
/// - Missing file:// prefix (returns None)
pub fn parse_file_uri(uri: &str) -> Option<PathBuf> {
    if !uri.starts_with("file://") {
        return None;
    }
    let path_str = uri.strip_prefix("file://")?;
    if path_str.is_empty() {
        return None;
    }
    // URL-decode the path
    let decoded = urlencoding::decode(path_str).ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

/// Encode a filesystem path as a file:// URI.
///
/// Properly encodes special characters like spaces, unicode, etc.
pub fn encode_file_uri(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    let mut out = String::with_capacity(path_str.len() + 7);
    out.push_str("file://");
    for &b in path_str.as_bytes() {
        let keep = matches!(
            b,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
        );
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

/// Malformed URI test cases for edge case testing.
pub mod malformed_uris {
    /// URIs that should be rejected (return None from parse_file_uri)
    pub const INVALID: &[&str] = &[
        "",                         // Empty
        "file://",                  // No path (missing slash after authority)
        "http://localhost/path",    // Wrong scheme
        "https://example.com/file", // Wrong scheme
        "ftp://server/file",        // Wrong scheme
        "file:",                    // Incomplete
        "file:/",                   // Incomplete (only one slash)
    ];

    /// URIs that might look valid but have edge cases
    pub const EDGE_CASES: &[(&str, Option<&str>)] = &[
        ("file:///tmp/test", Some("/tmp/test")), // Triple slash (valid)
        ("file:///", Some("/")),                 // Root directory (valid)
        ("file:///tmp/test%20file", Some("/tmp/test file")), // URL-encoded space
        ("file:///tmp/%E2%9C%93", Some("/tmp/✓")), // URL-encoded unicode
        ("file:///tmp/a%2Fb", Some("/tmp/a/b")), // Encoded slash (debatable)
        ("file:///C:/Windows", Some("/C:/Windows")), // Windows-style path on Unix
    ];
}
