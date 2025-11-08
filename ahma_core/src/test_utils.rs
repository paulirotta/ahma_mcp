use crate::client::Client;
use anyhow::Result;
use std::time::Duration;
use tempfile::TempDir;

pub async fn setup_mcp_service_with_client() -> Result<(TempDir, Client)> {
    // Create a temporary directory for tool configs
    let temp_dir = tempfile::tempdir()?;
    let tools_dir = temp_dir.path();
    let tool_config_path = tools_dir.join("long_running_async.json");

    let tool_config_content = r#"
    {
        "name": "long_running_async",
        "description": "A long running async command",
        "command": "sleep",
        "timeout_seconds": 30,
        "synchronous": false,
        "enabled": true,
        "subcommand": [
            {
                "name": "default",
                "description": "sleeps for a given duration",
                "positional_args": [
                    {
                        "name": "duration",
                        "option_type": "string",
                        "description": "duration to sleep",
                        "required": true
                    }
                ]
            }
        ]
    }
    "#;
    std::fs::write(&tool_config_path, tool_config_content)?;

    let mut client = Client::new();
    client
        .start_process(Some(tools_dir.to_str().unwrap()))
        .await?;

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(
        crate::constants::SEQUENCE_STEP_DELAY_MS,
    ))
    .await;

    Ok((temp_dir, client))
}
