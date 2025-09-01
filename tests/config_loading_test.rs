#[cfg(test)]
mod config_tests {
    use ahma_mcp::config::load_tool_configs;

    #[test]
    fn test_config_loading() {
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
                panic!("Configuration loading failed: {}", e);
            }
        }
    }
}
