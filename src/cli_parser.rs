use anyhow::{Context, Result};
use regex::Regex;
use std::process::Command;

/// Represents a parsed command-line option.
#[derive(Debug, Clone, PartialEq)]
pub struct CliOption {
    /// Short flag (e.g., 'h' for -h)
    pub short: Option<char>,
    /// Long flag (e.g., "help" for --help)
    pub long: Option<String>,
    /// Description of the option
    pub description: String,
    /// Whether this option takes a value
    pub takes_value: bool,
    /// Whether this option can be repeated multiple times
    pub multiple: bool,
}

/// Represents a parsed subcommand.
#[derive(Debug, Clone, PartialEq)]
pub struct CliSubcommand {
    /// Name of the subcommand
    pub name: String,
    /// Description of the subcommand
    pub description: String,
    /// Options specific to this subcommand
    pub options: Vec<CliOption>,
}

/// Represents the parsed structure of a CLI tool.
#[derive(Debug, Clone)]
pub struct CliStructure {
    /// Name of the tool
    pub tool_name: String,
    /// Global options available to all subcommands
    pub global_options: Vec<CliOption>,
    /// Available subcommands
    pub subcommands: Vec<CliSubcommand>,
    /// Whether the tool supports --help
    pub has_help: bool,
    /// Whether the tool supports --version
    pub has_version: bool,
}

impl CliStructure {
    /// Create a new CLI structure for a tool.
    pub fn new(tool_name: String) -> Self {
        CliStructure {
            tool_name,
            global_options: Vec::new(),
            subcommands: Vec::new(),
            has_help: false,
            has_version: false,
        }
    }

    /// Get an option by its long or short name.
    pub fn get_global_option(&self, name: &str) -> Option<&CliOption> {
        self.global_options.iter().find(|opt| {
            opt.long.as_deref() == Some(name)
                || opt.short.map(|c| c.to_string()) == Some(name.to_string())
        })
    }

    /// Get a subcommand by name.
    pub fn get_subcommand(&self, name: &str) -> Option<&CliSubcommand> {
        self.subcommands.iter().find(|cmd| cmd.name == name)
    }
}

/// Parses CLI tool help output to extract command structure.
#[derive(Debug)]
pub struct CliParser {
    subcommand_regex: Regex,
}

impl CliParser {
    /// Create a new CLI parser with default regex patterns.
    pub fn new() -> Result<Self> {
        Ok(CliParser {
            // Matches subcommands in help output - requires at least 3 spaces before command
            // and must have description after whitespace. Accept optional aliases separated by commas
            // Example matches:
            //   "    build, b    Compile the current package" -> name: build, desc: Compile the current package
            //   "    test       Run tests" -> name: test, desc: Run tests
            //   "    long-name, ln   Description" -> name: long-name, desc: Description
            subcommand_regex: Regex::new(
                r"^\s{2,}([A-Za-z0-9][A-Za-z0-9_-]*)\s*(?:,\s*[A-Za-z0-9][A-Za-z0-9_-]*)*(?:\s{2,}|\t+)(.+)",
            )?,
        })
    }

    /// Parse help output from a CLI tool.
    pub fn parse_help_output(&self, tool_name: &str, help_output: &str) -> Result<CliStructure> {
        let mut structure = CliStructure::new(tool_name.to_string());

        let lines: Vec<&str> = help_output.lines().collect();
        let mut current_section = "";
        let mut i = 0;

        while i < lines.len() {
            let raw_line = lines[i];
            let line = raw_line.trim();

            // Skip empty lines
            if line.is_empty() {
                i += 1;
                continue;
            }

            // Detect sections
            if line.to_lowercase().contains("options:") || line.to_lowercase().contains("flags:") {
                current_section = "options";
                i += 1;
                continue;
            }

            if line.to_lowercase().contains("commands:")
                || line.to_lowercase().contains("subcommands:")
                || line.to_lowercase().contains("commands used")
                || line.to_lowercase().contains("git commands")
            {
                current_section = "subcommands";
                i += 1;
                continue;
            }

            // Reset section on major headers
            if line.starts_with(char::is_uppercase) && line.ends_with(':') {
                current_section = "";
                i += 1;
                continue;
            }

            // Parse based on current section
            match current_section {
                "options" => {
                    if let Some(option) = self.parse_option_line(line)? {
                        structure.global_options.push(option);

                        // Check for help and version flags
                        if let Some(ref long) = structure.global_options.last().unwrap().long {
                            if long == "help" {
                                structure.has_help = true;
                            } else if long == "version" {
                                structure.has_version = true;
                            }
                        }
                    }
                }
                "subcommands" => {
                    if let Some(subcommand) = self.parse_subcommand_line(raw_line)? {
                        structure.subcommands.push(subcommand);
                    }
                }
                _ => {
                    // Some CLIs (like cargo) list subcommands without an explicit header.
                    // Try to parse subcommand lines opportunistically.
                    if let Some(subcommand) = self.parse_subcommand_line(raw_line)? {
                        structure.subcommands.push(subcommand);
                    } else if line.trim_start().starts_with('-')
                        && let Some(option) = self.parse_option_line(raw_line)?
                    {
                        // Fallback: parse as option if it looks like one
                        structure.global_options.push(option);

                        // Check for help and version flags
                        if let Some(ref long) = structure.global_options.last().unwrap().long {
                            if long == "help" {
                                structure.has_help = true;
                            } else if long == "version" {
                                structure.has_version = true;
                            }
                        }
                    }
                }
            }

            i += 1;
        }

        Ok(structure)
    }

    /// Parse a single line that should contain an option.
    pub fn parse_option_line(&self, line: &str) -> Result<Option<CliOption>> {
        let trimmed = line.trim();

        // Skip lines that don't start with '-'
        if !trimmed.starts_with('-') {
            return Ok(None);
        }

        let mut short = None;
        let mut long = None;
        let takes_value = line.contains('<') && line.contains('>');

        // Use multiple regex patterns to handle different formats
        let patterns = vec![
            // Pattern 1: -a, --all   description
            r"^\s*(-[a-zA-Z]),\s*(--[a-zA-Z0-9-_]+)\s+(.*)$",
            // Pattern 2: --long-option   description (no short)
            r"^\s*(--[a-zA-Z0-9-_]+)\s+(.*)$",
            // Pattern 3: -a   description (no long)
            r"^\s*(-[a-zA-Z])\s+(.*)$",
            // Pattern 4: --config-env=<name>=<envvar>  description (with =)
            r"^\s*(--[a-zA-Z0-9-_]+)=\S+\s+(.*)$",
        ];

        for pattern_str in &patterns {
            if let Ok(re) = Regex::new(pattern_str)
                && let Some(captures) = re.captures(line)
            {
                let description = captures
                    .get(captures.len() - 1)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();

                match captures.len() {
                    4 => {
                        // Pattern 1: -a, --all description
                        if let Some(short_match) = captures.get(1) {
                            short = short_match.as_str().chars().nth(1);
                        }
                        if let Some(long_match) = captures.get(2) {
                            long = Some(long_match.as_str()[2..].to_string());
                        }
                    }
                    3 => {
                        // Pattern 2 or 3 or 4
                        let flag_match = captures.get(1).unwrap();
                        let flag = flag_match.as_str();
                        if flag.starts_with("--") {
                            // Long flag
                            let long_flag = if flag.contains('=') {
                                flag.split('=').next().unwrap()
                            } else {
                                flag
                            };
                            long = Some(long_flag[2..].to_string());
                        } else {
                            // Short flag
                            short = flag.chars().nth(1);
                        }
                    }
                    _ => continue,
                }

                // Skip if we don't have at least one flag
                if short.is_none() && long.is_none() {
                    continue;
                }

                let multiple = description.to_lowercase().contains("multiple")
                    || description.to_lowercase().contains("repeat");

                return Ok(Some(CliOption {
                    short,
                    long,
                    description,
                    takes_value,
                    multiple,
                }));
            }
        }

        Ok(None)
    }

    /// Parse a single line that should contain a subcommand.
    pub fn parse_subcommand_line(&self, line: &str) -> Result<Option<CliSubcommand>> {
        if let Some(captures) = self.subcommand_regex.captures(line) {
            let name = captures
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let description = captures
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            // Skip if name is empty or looks like an option
            if name.is_empty() || name.starts_with('-') {
                return Ok(None);
            }

            Ok(Some(CliSubcommand {
                name,
                description,
                options: Vec::new(), // Subcommand-specific options would need separate parsing
            }))
        } else {
            Ok(None)
        }
    }

    /// Get help output from a CLI tool by running it with --help.
    pub fn get_help_output(&self, tool_name: &str) -> Result<String> {
        let output = Command::new(tool_name)
            .arg("--help")
            .output()
            .with_context(|| format!("Failed to execute '{} --help'", tool_name))?;

        if !output.status.success() {
            // Try -h as fallback
            let output = Command::new(tool_name)
                .arg("-h")
                .output()
                .with_context(|| format!("Failed to execute '{} -h'", tool_name))?;

            if !output.status.success() {
                anyhow::bail!("Tool '{}' does not support --help or -h", tool_name);
            }

            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Parse a CLI tool by running --help and parsing the output.
    pub fn parse_tool(&self, tool_name: &str) -> Result<CliStructure> {
        let help_output = self.get_help_output(tool_name)?;
        self.parse_help_output(tool_name, &help_output)
    }

    /// Parse a CLI tool using configuration.
    pub async fn parse_tool_with_config(
        &self,
        config: &crate::config::Config,
    ) -> Result<CliStructure> {
        let help_output = self.get_help_output_with_config(config).await?;
        let command = config.command.as_deref().unwrap_or(&config.tool_name);
        self.parse_help_output(command, &help_output)
    }

    /// Get help output using configuration.
    async fn get_help_output_with_config(&self, config: &crate::config::Config) -> Result<String> {
        let help_args = vec!["--help".to_string()]; // Default help args - no help_args field in config
        let command = config.command.as_deref().unwrap_or(&config.tool_name);

        let output = tokio::process::Command::new(command)
            .args(&help_args)
            .output()
            .await
            .context(format!("Failed to run {} with help args", command))?;

        if !output.status.success() {
            // Try stderr if stdout is empty
            if output.stdout.is_empty() && !output.stderr.is_empty() {
                Ok(String::from_utf8_lossy(&output.stderr).to_string())
            } else {
                Err(anyhow::anyhow!(
                    "Help command failed with status: {}",
                    output.status
                ))
            }
        } else {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }
    }
}

impl Default for CliParser {
    fn default() -> Self {
        Self::new().expect("Failed to create default CLI parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_ls_help() {
        let parser = CliParser::new().unwrap();

        // Sample ls --help output
        let help_output = r#"
Usage: ls [OPTION]... [FILE]...
List information about the FILEs (the current directory by default).

  -a, --all                  do not ignore entries starting with .
  -l                         use a long listing format
  -h, --human-readable       with -l and/or -s, print human readable sizes
  -r, --reverse              reverse order while sorting
  -t                         sort by time, newest first
  -S                         sort by file size, largest first
      --help                 display this help and exit
      --version              output version information and exit
"#;

        let structure = parser.parse_help_output("ls", help_output).unwrap();

        assert_eq!(structure.tool_name, "ls");
        assert!(structure.has_help);
        assert!(structure.has_version);
        assert!(!structure.global_options.is_empty());

        // Check for specific options
        let all_option = structure.get_global_option("all").unwrap();
        assert_eq!(all_option.short, Some('a'));
        assert_eq!(all_option.long, Some("all".to_string()));
        assert!(!all_option.takes_value);

        let human_option = structure.get_global_option("human-readable").unwrap();
        assert_eq!(human_option.short, Some('h'));
        assert_eq!(human_option.long, Some("human-readable".to_string()));
    }

    #[test]
    fn test_parse_git_help_with_subcommands() {
        let parser = CliParser::new().unwrap();

        // Simplified git --help output
        let help_output = r#"
usage: git [--version] [--help] [-C <path>] [--exec-path[=<path>]]
           [--html-path] [--man-path] [--info-path]
           [-p | --paginate | -P | --no-pager] [--no-replace-objects] [--bare]
           [--git-dir=<path>] [--work-tree=<path>] [--namespace=<name>]
           [--super-prefix=<path>] [--config-env=<name>=<envvar>]
           <command> [<args>]

These are common Git commands used in various situations:

   add        Add file contents to the index
   branch     List, create, or delete branches
   checkout   Switch branches or restore working tree files
   clone      Clone a repository into a new directory
   commit     Record changes to the repository
   diff       Show changes between commits, commit and working tree, etc
   merge      Join two or more development histories together
   pull       Fetch from and integrate with another repository or a local branch
   push       Update remote refs along with associated objects
   status     Show the working tree status

See 'git help <command>' for more information on a specific command.
"#;

        let structure = parser.parse_help_output("git", help_output).unwrap();

        assert_eq!(structure.tool_name, "git");
        assert!(!structure.subcommands.is_empty());

        // Check for specific subcommands
        let add_cmd = structure.get_subcommand("add").unwrap();
        assert_eq!(add_cmd.name, "add");
        assert!(add_cmd.description.contains("Add file contents"));

        let commit_cmd = structure.get_subcommand("commit").unwrap();
        assert_eq!(commit_cmd.name, "commit");
        assert!(commit_cmd.description.contains("Record changes"));
    }

    #[test]
    fn test_parse_option_line() {
        let parser = CliParser::new().unwrap();

        // Test various option formats
        let test_cases = vec![
            (
                "  -a, --all                  do not ignore entries starting with .",
                Some('a'),
                Some("all"),
                false,
            ),
            (
                "  -h, --human-readable       with -l, print human readable sizes",
                Some('h'),
                Some("human-readable"),
                false,
            ),
            (
                "      --help                 display this help and exit",
                None,
                Some("help"),
                false,
            ),
            (
                "  -C <path>                  run as if git was started in <path>",
                Some('C'),
                None,
                true,
            ),
            (
                "      --config-env=<name>=<envvar>  config environment",
                None,
                Some("config-env"),
                true,
            ),
        ];

        for (line, expected_short, expected_long, expected_takes_value) in test_cases {
            let option = parser.parse_option_line(line).unwrap().unwrap();
            assert_eq!(option.short, expected_short, "Failed for line: {}", line);
            assert_eq!(
                option.long.as_deref(),
                expected_long,
                "Failed for line: {}",
                line
            );
            assert_eq!(
                option.takes_value, expected_takes_value,
                "Failed for line: {}",
                line
            );
        }
    }

    #[test]
    fn test_parse_subcommand_line() {
        let parser = CliParser::new().unwrap();

        let test_cases = vec![
            (
                "   add        Add file contents to the index",
                "add",
                "Add file contents to the index",
            ),
            (
                "   commit     Record changes to the repository",
                "commit",
                "Record changes to the repository",
            ),
            (
                "   long-command-name    Does something with a long name",
                "long-command-name",
                "Does something with a long name",
            ),
            (
                "    build, b    Compile the current package",
                "build",
                "Compile the current package",
            ),
            (
                "    run, r      Run a binary or example of the local package",
                "run",
                "Run a binary or example of the local package",
            ),
        ];

        for (line, expected_name, expected_desc) in test_cases {
            let subcommand = parser.parse_subcommand_line(line).unwrap().unwrap();
            assert_eq!(subcommand.name, expected_name);
            assert_eq!(subcommand.description, expected_desc);
        }
    }

    #[test]
    fn test_empty_and_invalid_lines() {
        let parser = CliParser::new().unwrap();

        // Empty lines and invalid formats should return None
        assert!(parser.parse_option_line("").unwrap().is_none());
        assert!(parser.parse_option_line("   ").unwrap().is_none());
        assert!(
            parser
                .parse_option_line("This is not an option line")
                .unwrap()
                .is_none()
        );

        assert!(parser.parse_subcommand_line("").unwrap().is_none());
        assert!(parser.parse_subcommand_line("   ").unwrap().is_none());
        assert!(
            parser
                .parse_subcommand_line("  --this-looks-like-an-option")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_cli_structure_helpers() {
        let mut structure = CliStructure::new("test".to_string());

        structure.global_options.push(CliOption {
            short: Some('v'),
            long: Some("verbose".to_string()),
            description: "Enable verbose output".to_string(),
            takes_value: false,
            multiple: false,
        });

        structure.subcommands.push(CliSubcommand {
            name: "build".to_string(),
            description: "Build the project".to_string(),
            options: Vec::new(),
        });

        // Test option lookup
        assert!(structure.get_global_option("verbose").is_some());
        assert!(structure.get_global_option("v").is_some());
        assert!(structure.get_global_option("nonexistent").is_none());

        // Test subcommand lookup
        assert!(structure.get_subcommand("build").is_some());
        assert!(structure.get_subcommand("nonexistent").is_none());
    }
}
