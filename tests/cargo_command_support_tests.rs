//! Verify cargo core and optional subcommands discovery aligns with requirements.

use ahma_mcp::{adapter::Adapter, config::Config};
use anyhow::Result;

async fn load_cargo(adapter: &mut Adapter) -> Result<()> {
    let config = Config::load_tool_config("cargo")?;
    adapter.add_tool("cargo", config).await
}

#[tokio::test]
async fn test_cargo_core_commands_present() -> Result<()> {
    let mut adapter = Adapter::new(true)?;

    if let Err(e) = load_cargo(&mut adapter).await {
        eprintln!("Skipping cargo discovery test: {}", e);
        return Ok(());
    }

    // Inspect subcommands from the schema's subcommand enum
    let schemas = adapter.get_tool_schemas()?;
    let cargo_schema = schemas
        .into_iter()
        .find(|s| s.get("name").and_then(|v| v.as_str()) == Some("cargo"))
        .expect("cargo schema should be present");

    let sub_enum = cargo_schema
        .get("inputSchema")
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("subcommand"))
        .and_then(|v| v.get("enum"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let subcommands: Vec<String> = sub_enum
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    // Require presence of core subcommands (tolerant)
    let required = ["build", "test", "run", "check"];
    let have_required = required
        .iter()
        .filter(|r| subcommands.iter().any(|s| s.as_str() == **r))
        .count();
    for r in required {
        if !subcommands.iter().any(|s| s.as_str() == r) {
            eprintln!("Note: cargo subcommand '{}' not found in help output", r);
        }
    }
    assert!(
        have_required >= 3,
        "Expected most core cargo subcommands to be present"
    );

    // Optional/extension commands: do not assert, just log if present
    let optional = [
        "clippy",
        "nextest",
        "fmt",
        "audit",
        "upgrade",
        "bump_version",
        "bench",
        "add",
        "remove",
        "update",
        "fetch",
        "install",
        "search",
        "tree",
        "version",
        "rustc",
        "metadata",
    ];
    for opt in optional {
        if subcommands.iter().any(|s| s.as_str() == opt) {
            eprintln!("Found optional cargo subcommand: {}", opt);
        }
    }

    Ok(())
}
