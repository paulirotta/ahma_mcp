use super::*;
use tempfile::tempdir;
use std::fs::File;
use std::io::Write;

#[tokio::test]
async fn test_load_config_non_existent() {
    let result = Config::load("non_existent_tool");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_load_config_success() {
    let dir = tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();
    let config_path = tools_dir.join("test_tool.toml");

    let mut file = File::create(&config_path).unwrap();
    file.write_all(b"
[tool]
name = \"test_tool\"
command = \"echo\"
description = \"A test tool\"
    ").unwrap();

    // To make this test work, we need to temporarily point our config loader to this directory.
    // For now, we'll assume the loader looks in a relative `tools` directory.
    // This will likely require modification of the `Config::load` function.
    // We will simulate this by changing the current directory for the test.
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let result = Config::load("test_tool");
    assert!(result.is_ok());
    let config = result.unwrap();
    assert_eq!(config.tool.name, "test_tool");
    assert_eq!(config.tool.command, "echo");

    // Change back to the original directory
    std::env::set_current_dir(original_dir).unwrap();
}
