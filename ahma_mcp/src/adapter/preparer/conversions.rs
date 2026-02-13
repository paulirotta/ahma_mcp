use anyhow::Result;
use serde_json::Value;

/// Resolves a boolean value from a JSON value.
/// Handles both native boolean values and string representations ("true"/"false").
pub(super) fn resolve_bool(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| value.as_str().map(|s| s.eq_ignore_ascii_case("true")))
}

/// Converts a serde_json::Value to a string, handling recursion.
pub(super) fn coerce_cli_value(value: &Value) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::Bool(b) => Ok(Some(b.to_string())),
        Value::Array(arr) => {
            let mut result = Vec::new();
            for item in arr {
                if let Some(s) = coerce_cli_value(item)? {
                    result.push(s);
                }
            }
            if result.is_empty() {
                return Ok(None);
            }
            Ok(Some(result.join(" ")))
        }
        // For other types like Object, we don't want to convert them to a string.
        _ => Ok(None),
    }
}

pub(super) fn is_reserved_runtime_key(key: &str) -> bool {
    matches!(
        key,
        "args" | "working_directory" | "execution_mode" | "timeout_seconds"
    )
}

/// Checks if a string contains characters that are problematic for shell argument passing
pub fn needs_file_handling(value: &str) -> bool {
    value.contains('\n')
        || value.contains('\r')
        || value.contains('\'')
        || value.contains('"')
        || value.contains('\\')
        || value.contains('`')
        || value.contains('$')
        || value.len() > 8192 // Also handle very long arguments via file
}

/// Formats an option name as a command-line flag.
///
/// If the option name already starts with a dash (e.g., "-name" for `find`),
/// it's used as-is. Otherwise, it's prefixed with "--" for standard long options.
pub fn format_option_flag(key: &str) -> String {
    if key.starts_with('-') {
        key.to_string()
    } else {
        format!("--{}", key)
    }
}

/// Prepare a string for shell argument passing by escaping special characters.
///
/// This function wraps the string in single quotes and handles any embedded single quotes.
///
/// # Purpose
///
/// "Escaping" here means neutralizing special characters (like spaces, `$`, quotes, etc.)
/// so the shell treats the value as a single piece of text (a literal string) rather than
/// interpreting it as code or multiple arguments. This prevents "shell injection" attacks.
///
/// # When to use
///
/// Use this only as a fallback when you cannot use file-based data passing. Passing data
/// through temporary files is generally robust, but if you must construct a raw command
/// string with arguments, this function ensures those arguments are safe.
pub fn escape_shell_argument(value: &str) -> String {
    // Use single quotes and escape any embedded single quotes
    if value.contains('\'') {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    } else {
        format!("'{}'", value)
    }
}
