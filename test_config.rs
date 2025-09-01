use ahma_mcp::config::load_tool_configs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing configuration loading...");

    match load_tool_configs() {
        Ok(configs) => {
            println!("Successfully loaded {} configurations:", configs.len());
            for (name, config) in configs.iter() {
                println!("  - {}: {} subcommands", name, config.subcommand.len());
            }
        }
        Err(e) => {
            eprintln!("Failed to load configurations: {}", e);
            eprintln!("Error chain:");
            let mut source = e.source();
            while let Some(err) = source {
                eprintln!("  caused by: {}", err);
                source = err.source();
            }
        }
    }

    Ok(())
}
