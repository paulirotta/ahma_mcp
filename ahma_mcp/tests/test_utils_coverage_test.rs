//! Test coverage for test_utils module utility functions
//!
//! These tests verify the pure utility functions in test_utils
//! that are reusable across the test suite.

use ahma_mcp::test_utils::{self, strip_ansi};

#[test]
fn test_strip_ansi_removes_simple_color_codes() {
    let colored = "\x1b[31mred text\x1b[0m";
    let stripped = strip_ansi(colored);
    assert_eq!(stripped, "red text");
}

#[test]
fn test_strip_ansi_handles_bold_and_underline() {
    let styled = "\x1b[1mbold\x1b[0m and \x1b[4munderline\x1b[0m";
    let stripped = strip_ansi(styled);
    assert_eq!(stripped, "bold and underline");
}

#[test]
fn test_strip_ansi_handles_multiple_parameters() {
    // SGR with multiple parameters like \x1b[1;31m (bold red)
    let multi_param = "\x1b[1;31mbold red\x1b[0m";
    let stripped = strip_ansi(multi_param);
    assert_eq!(stripped, "bold red");
}

#[test]
fn test_strip_ansi_preserves_plain_text() {
    let plain = "Hello, world!";
    let stripped = strip_ansi(plain);
    assert_eq!(stripped, "Hello, world!");
}

#[test]
fn test_strip_ansi_handles_empty_string() {
    let empty = "";
    let stripped = strip_ansi(empty);
    assert_eq!(stripped, "");
}

#[test]
fn test_strip_ansi_handles_text_with_newlines() {
    let multiline = "\x1b[32mline1\x1b[0m\nline2\n\x1b[34mline3\x1b[0m";
    let stripped = strip_ansi(multiline);
    assert_eq!(stripped, "line1\nline2\nline3");
}

#[test]
fn test_strip_ansi_handles_256_color_codes() {
    // 256 color: \x1b[38;5;196m (foreground color 196)
    let color256 = "\x1b[38;5;196mred 256\x1b[0m";
    let stripped = strip_ansi(color256);
    assert_eq!(stripped, "red 256");
}

#[test]
fn test_strip_ansi_handles_true_color_codes() {
    // True color: \x1b[38;2;255;0;0m (RGB red)
    let truecolor = "\x1b[38;2;255;0;0mtrue red\x1b[0m";
    let stripped = strip_ansi(truecolor);
    assert_eq!(stripped, "true red");
}

#[test]
fn test_strip_ansi_handles_cursor_movement() {
    // Cursor up: \x1b[2A
    let cursor = "\x1b[2Atext after cursor up";
    let stripped = strip_ansi(cursor);
    assert_eq!(stripped, "text after cursor up");
}

#[test]
fn test_strip_ansi_handles_clear_screen() {
    // Clear screen: \x1b[2J
    let clear = "\x1b[2Jtext after clear";
    let stripped = strip_ansi(clear);
    assert_eq!(stripped, "text after clear");
}

#[test]
fn test_strip_ansi_handles_lone_escape() {
    // Lone ESC without CSI should be stripped
    let lone_esc = "before\x1bafter";
    let stripped = strip_ansi(lone_esc);
    assert_eq!(stripped, "beforeafter");
}

// Tests for contains_any
#[test]
fn test_contains_any_with_single_match() {
    assert!(test_utils::contains_any("hello world", &["world"]));
}

#[test]
fn test_contains_any_with_multiple_patterns_one_matches() {
    assert!(test_utils::contains_any(
        "hello world",
        &["foo", "bar", "world"]
    ));
}

#[test]
fn test_contains_any_with_no_matches() {
    assert!(!test_utils::contains_any("hello world", &["foo", "bar"]));
}

#[test]
fn test_contains_any_with_empty_patterns() {
    assert!(!test_utils::contains_any("hello world", &[]));
}

#[test]
fn test_contains_any_with_empty_string() {
    // Empty string contains empty pattern
    assert!(test_utils::contains_any("hello world", &[""]));
}

// Tests for contains_all
#[test]
fn test_contains_all_with_all_present() {
    assert!(test_utils::contains_all(
        "hello world foo bar",
        &["hello", "world"]
    ));
}

#[test]
fn test_contains_all_with_some_missing() {
    assert!(!test_utils::contains_all(
        "hello world",
        &["hello", "missing"]
    ));
}

#[test]
fn test_contains_all_with_empty_patterns() {
    // Empty pattern list - all patterns are present (vacuously true)
    assert!(test_utils::contains_all("hello world", &[]));
}

#[test]
fn test_contains_all_with_single_pattern() {
    assert!(test_utils::contains_all("hello world", &["hello"]));
    assert!(!test_utils::contains_all("hello world", &["missing"]));
}

// Tests for extract_tool_names
#[test]
fn test_extract_tool_names_with_loading_tool_lines() {
    let debug_output = r#"
INFO Loading tool: cargo
INFO Loading tool: git
INFO Other log message
"#;
    let tool_names = test_utils::extract_tool_names(debug_output);
    assert_eq!(tool_names, vec!["cargo", "git"]);
}

#[test]
fn test_extract_tool_names_with_tool_loaded_lines() {
    let debug_output = r#"
DEBUG Tool loaded: npm
DEBUG Tool loaded: yarn
"#;
    let tool_names = test_utils::extract_tool_names(debug_output);
    assert_eq!(tool_names, vec!["npm", "yarn"]);
}

#[test]
fn test_extract_tool_names_with_mixed_formats() {
    let debug_output = r#"
Loading tool: rustc
Tool loaded: clippy
Loading tool: fmt
"#;
    let tool_names = test_utils::extract_tool_names(debug_output);
    assert_eq!(tool_names, vec!["rustc", "clippy", "fmt"]);
}

#[test]
fn test_extract_tool_names_with_no_matches() {
    let debug_output = "Some random log output\nNo tools here";
    let tool_names = test_utils::extract_tool_names(debug_output);
    assert!(tool_names.is_empty());
}

#[test]
fn test_extract_tool_names_with_empty_input() {
    let tool_names = test_utils::extract_tool_names("");
    assert!(tool_names.is_empty());
}

// Tests for file_exists and dir_exists
#[tokio::test]
async fn test_file_exists_with_existing_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&file_path, "test content").await.unwrap();

    assert!(test_utils::file_exists(&file_path).await);
}

#[tokio::test]
async fn test_file_exists_with_non_existing_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("non_existing.txt");

    assert!(!test_utils::file_exists(&file_path).await);
}

#[tokio::test]
async fn test_file_exists_with_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    // temp_dir.path() is a directory, not a file
    assert!(!test_utils::file_exists(temp_dir.path()).await);
}

#[tokio::test]
async fn test_dir_exists_with_existing_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    assert!(test_utils::dir_exists(temp_dir.path()).await);
}

#[tokio::test]
async fn test_dir_exists_with_non_existing_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let dir_path = temp_dir.path().join("non_existing_dir");

    assert!(!test_utils::dir_exists(&dir_path).await);
}

#[tokio::test]
async fn test_dir_exists_with_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&file_path, "test content").await.unwrap();

    // file_path is a file, not a directory
    assert!(!test_utils::dir_exists(&file_path).await);
}

// Tests for read_file_contents and write_file_contents
#[tokio::test]
async fn test_read_write_file_contents() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    let content = "Hello, World!\nSecond line.";

    test_utils::write_file_contents(&file_path, content)
        .await
        .unwrap();
    let read_content = test_utils::read_file_contents(&file_path).await.unwrap();

    assert_eq!(read_content, content);
}

#[tokio::test]
async fn test_write_file_contents_overwrites() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    test_utils::write_file_contents(&file_path, "first")
        .await
        .unwrap();
    test_utils::write_file_contents(&file_path, "second")
        .await
        .unwrap();

    let content = test_utils::read_file_contents(&file_path).await.unwrap();
    assert_eq!(content, "second");
}

#[tokio::test]
async fn test_read_file_contents_non_existing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("non_existing.txt");

    let result = test_utils::read_file_contents(&file_path).await;
    assert!(result.is_err());
}
