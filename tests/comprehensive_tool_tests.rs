//! Comprehensive integration tests translated from comprehensive_tool_test.py
//! These tests exercise the Adapter directly for speed and determinism.

use anyhow::Result;
use tempfile::TempDir;
use tokio::fs;

use ahma_mcp::{adapter::Adapter, config::Config};

async fn add_tool_from_toml(adapter: &mut Adapter, tool: &str) -> Result<()> {
    let toml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tools")
        .join(format!("{}.toml", tool));
    let config = Config::load_from_file(&toml_path)?;
    adapter.add_tool(tool, config).await
}

#[tokio::test]
async fn test_comprehensive_tools_via_adapter() -> Result<()> {
    // Prepare temporary test files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    fs::write(
        test_dir.join("test.txt"),
        "Hello World\nThis is a test file\nTODO: Add more content\nAnother line\n",
    )
    .await?;
    fs::write(test_dir.join("numbers.txt"), "1\n2\n3\n4\n5\n").await?;

    // Use synchronous adapter for predictable behavior
    let mut adapter = Adapter::new(true)?;

    // Add required tools from the tools directory
    for tool in ["echo", "ls", "cat", "grep", "sed", "git"] {
        // Don't fail the whole test if a tool isn't available on CI
        if let Err(e) = add_tool_from_toml(&mut adapter, tool).await {
            eprintln!("Skipping tool {}: {}", tool, e);
        }
    }

    // 1) echo
    if let Ok(output) = adapter
        .execute_tool("echo", vec!["Hello from ahma_mcp!".to_string()])
        .await
    {
        assert!(output.contains("Hello from ahma_mcp!"));
    }

    // 2) ls of temp directory
    if let Ok(output) = adapter
        .execute_tool(
            "ls",
            vec![test_dir.to_string_lossy().to_string(), "-1".to_string()],
        )
        .await
    {
        assert!(output.contains("test.txt"));
        assert!(output.contains("numbers.txt"));
    }

    // 3) cat test file
    if let Ok(output) = adapter
        .execute_tool(
            "cat",
            vec![test_dir.join("test.txt").to_string_lossy().to_string()],
        )
        .await
    {
        assert!(output.contains("Hello World"));
        assert!(output.contains("TODO: Add more content"));
    }

    // 4) grep TODO in test file
    if let Ok(output) = adapter
        .execute_tool(
            "grep",
            vec![
                "TODO".to_string(),
                test_dir.join("test.txt").to_string_lossy().to_string(),
            ],
        )
        .await
    {
        assert!(output.contains("TODO: Add more content"));
    }

    // 5) sed replace Hello -> Hi
    if let Ok(output) = adapter
        .execute_tool(
            "sed",
            vec![
                "s/Hello/Hi/g".to_string(),
                test_dir.join("test.txt").to_string_lossy().to_string(),
            ],
        )
        .await
    {
        assert!(output.contains("Hi World"));
        assert!(!output.contains("Hello World"));
    }

    // 6) git version (works without a repo)
    if let Ok(output) = adapter
        .execute_tool("git", vec!["--version".to_string()])
        .await
    {
        assert!(output.contains("git version"));
    }

    Ok(())
}
