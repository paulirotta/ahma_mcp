#[cfg(test)]
mod test_temp {
    use crate::cli_parser::CliParser;

    #[test]
    fn debug_git_help_full() {
        let parser = CliParser::new().unwrap();

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

        eprintln!("Tool name: {}", structure.tool_name);
        eprintln!(
            "Number of subcommands found: {}",
            structure.subcommands.len()
        );
        for subcommand in &structure.subcommands {
            eprintln!("  {}: {}", subcommand.name, subcommand.description);
        }

        panic!("Debug output shown above");
    }
}
