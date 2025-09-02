mod common;

use anyhow::Result;
use common::test_client::new_client;
use rmcp::model::CallToolRequestParam;
use serde_json::Map;
use std::borrow::Cow;

#[tokio::test]
async fn test_ls_tool_should_not_add_undefined_path_parameter() -> Result<()> {
    // ARRANGE: Set up test client to execute ls tool
    let client = new_client(Some("tools")).await?;

    // ACT: Execute ls tool without any parameters (empty arguments map)
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("ls_default"),
        arguments: Some(Map::new()), // Empty arguments - should not add --path=.
    };

    let result = client.call_tool(call_param).await;

    // ASSERT: Verify the command construction does not include --path=.
    // The actual command should be "ls" not "ls --path=."
    // This test should FAIL initially to demonstrate the bug
    match result {
        Ok(response) => {
            if let Some(content) = response.content.first() {
                if let Some(text_content) = content.as_text() {
                    let output = &text_content.text;
                    
                    // Check that we don't see the incorrect --path=. argument
                    // (This assertion should fail initially, confirming the bug exists)
                    assert!(
                        !output.contains("--path=."),
                        "ls command should not include --path=. parameter when no path is specified. Found: {}",
                        output
                    );
                    
                    // Additional check: verify it's not trying to use invalid --path option
                    // If this fails, it confirms our bug exists
                    assert!(
                        !output.contains("ls: unrecognized option '--path'") &&
                        !output.contains("ls: illegal option -- -") &&
                        !output.contains("ls: invalid option"),
                        "ls command appears to be using invalid --path option. Output: {}",
                        output
                    );
                }
            }
        },
        Err(e) => {
            // If the tool call fails, check if it's due to the --path=. bug
            let error_str = format!("{:?}", e);
            if error_str.contains("unrecognized option") || error_str.contains("illegal option") {
                panic!("ls tool failed due to invalid --path=. parameter: {}", error_str);
            }
            return Err(e.into());
        }
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test] 
async fn test_ls_tool_executes_plain_ls_command() -> Result<()> {
    // ARRANGE: Set up test client  
    let client = new_client(Some("tools")).await?;

    // ACT: Execute ls tool with empty parameters
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("ls_default"),
        arguments: Some(Map::new()),
    };

    let result = client.call_tool(call_param).await?;

    // ASSERT: Should execute successfully without --path=. error
    // This is testing that the command construction creates valid shell command
    if let Some(content) = result.content.first() {
        if let Some(text_content) = content.as_text() {
            let output = &text_content.text;
            
            // Verify command succeeded (no error about unrecognized option)
            assert!(
                !output.contains("unrecognized option") && 
                !output.contains("illegal option") &&
                !output.contains("invalid option"),
                "ls command should execute successfully, got: {}",
                output
            );
            
            // Verify we get directory listing output (basic sanity check)
            assert!(
                !output.is_empty(),
                "ls command should produce directory listing output"
            );
        }
    }

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn test_ls_tool_with_valid_options() -> Result<()> {
    // ARRANGE: Set up test client with tools directory
    let client = new_client(Some("tools")).await?;

    // ACT: Execute ls tool (should work without adding undefined --path)
    let call_param = CallToolRequestParam {
        name: Cow::Borrowed("ls_default"),
        arguments: Some(Map::new()),
    };

    let result = client.call_tool(call_param).await?;

    // ASSERT: Should complete successfully
    if let Some(content) = result.content.first() {
        if let Some(text_content) = content.as_text() {
            let output = &text_content.text;
            
            // Basic validation that command executed
            assert!(
                !output.contains("command not found"),
                "ls command should be available, got: {}",
                output
            );
        }
    }

    client.cancel().await?;
    Ok(())
}
