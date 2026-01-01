use ahma_core::test_utils::test_client::new_client_with_args;
use anyhow::Result;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

#[tokio::test]
async fn test_dynamic_config_reload() -> Result<()> {
    // 1. Create a temporary directory for tools
    let temp_dir = tempdir()?;
    let tools_dir = temp_dir.path().to_path_buf();

    // 2. Create an initial tool
    let initial_tool = r#"
{
    "name": "initial_tool",
    "description": "Initial tool",
    "command": "echo",
    "timeout_seconds": 10,
    "synchronous": true,
    "enabled": true,
    "subcommand": [
        {
            "name": "default",
            "description": "Default subcommand"
        }
    ]
}
"#;
    fs::write(tools_dir.join("initial_tool.json"), initial_tool)?;

    // 3. Start the MCP server using the test client
    // We pass the tools_dir to the server
    let mut client = new_client_with_args(Some(tools_dir.to_str().unwrap()), &[]).await?;

    // 4. Verify initial tool is present
    let tools = client.list_tools(None).await?;
    assert!(tools.tools.iter().any(|t| t.name == "initial_tool"));
    assert!(!tools.tools.iter().any(|t| t.name == "new_tool"));

    // 5. Add a new tool JSON file
    let new_tool = r#"
{
    "name": "new_tool",
    "description": "New tool added dynamically",
    "command": "echo",
    "timeout_seconds": 10,
    "synchronous": true,
    "enabled": true,
    "subcommand": [
        {
            "name": "default",
            "description": "Default subcommand"
        }
    ]
}
"#;
    fs::write(tools_dir.join("new_tool.json"), new_tool)?;

    // 6. Wait for the watcher to detect the change and reload (debounce is 200ms)
    // We'll wait a bit longer to be safe
    sleep(Duration::from_millis(1000)).await;

    // 7. Verify new tool is now present
    let tools = client.list_tools(None).await?;
    assert!(tools.tools.iter().any(|t| t.name == "initial_tool"));
    assert!(tools.tools.iter().any(|t| t.name == "new_tool"), "New tool should be present after reload");

    // 8. Modify an existing tool
    let modified_tool = r#"
{
    "name": "initial_tool",
    "description": "Modified initial tool",
    "command": "echo",
    "timeout_seconds": 10,
    "synchronous": true,
    "enabled": true,
    "subcommand": [
        {
            "name": "default",
            "description": "Default subcommand"
        }
    ]
}
"#;
    fs::write(tools_dir.join("initial_tool.json"), modified_tool)?;
    sleep(Duration::from_millis(1000)).await;

    let tools = client.list_tools(None).await?;
    let initial = tools.tools.iter().find(|t| t.name == "initial_tool").unwrap();
    assert_eq!(initial.description, Some("Modified initial tool".into()));

    // 9. Remove a tool
    fs::remove_file(tools_dir.join("new_tool.json"))?;
    sleep(Duration::from_millis(1000)).await;

    let tools = client.list_tools(None).await?;
    assert!(!tools.tools.iter().any(|t| t.name == "new_tool"), "New tool should be removed after file deletion");

    Ok(())
}
