use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use toml;

#[derive(Deserialize, Debug)]
pub struct ToolConfig {
    pub name: String,
    pub command: String,
    pub description: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub tool: ToolConfig,
}

impl Config {
    pub fn load(tool_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = PathBuf::from("tools").join(format!("{}.toml", tool_name));
        let content = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
