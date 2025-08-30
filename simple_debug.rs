use std::fs;

fn main() {
    // Simple debug - check what the test help text has
    let help_output = r#"usage: git [--version] [--help] [-C <path>] [-c <name>=<value>]
           [--exec-path[=<path>]] [--html-path] [--man-path] [--info-path]
           [-p | --paginate | -P | --no-pager] [--no-replace-objects] [--bare]
           [--git-dir=<path>] [--work-tree=<path>] [--namespace=<name>]
           <command> [<args>]

These are common Git commands used in various situations:

start a working area (see also: git help tutorial)
   clone      Clone a repository into a new directory
   init       Create an empty Git repository or reinitialize an existing one

work on the current change (see also: git help everyday)  
   add        Add file contents to the index
   mv         Move or rename a file, a directory, or a symlink
   reset      Reset current HEAD to the specified state
   rm         Remove files from the working tree and from the index

examine the history and state (see also: git help revisions)
   bisect     Use binary search to find the commit that introduced a bug
   diff       Show changes between commits, commit and working tree, etc
   grep       Print lines matching a pattern
   log        Show commit logs
   show       Show various types of objects
   status     Show the working tree status

grow, mark and tweak your common history
   branch     List, create, or delete branches
   commit     Record changes to the repository
   merge      Join two or more development histories together
   pull       Fetch from and integrate with another repository or a local branch
   push       Update remote refs along with associated objects
   status     Show the working tree status

See 'git help <command>' for more information on a specific command.
"#;

    let regex = regex::Regex::new(r"^\s{3,}([a-zA-Z0-9-_]+)\s+(.+)").unwrap();
    let mut count = 0;

    for line in help_output.lines() {
        if let Some(captures) = regex.captures(line) {
            let name = captures.get(1).unwrap().as_str();
            let description = captures.get(2).unwrap().as_str();
            count += 1;
            println!("Found subcommand: '{}' - '{}'", name, description);
        }
    }

    println!("Total subcommands found: {}", count);
}
