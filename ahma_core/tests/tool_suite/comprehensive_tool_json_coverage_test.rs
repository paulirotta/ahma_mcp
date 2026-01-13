//! Comprehensive coverage tests for all .ahma/*.json tool definitions.
//!
//! This test module ensures that all JSON tool definitions are:
//! 1. Properly loadable and parseable
//! 2. Structurally correct with all required fields
//! 3. Have subcommands/options that follow naming conventions
//! 4. Have correct force_synchronous settings
//!
//! These tests catch regressions early when tool definitions are modified.

use ahma_core::test_utils as common;

use ahma_core::config::{SubcommandConfig, ToolConfig, load_tool_configs};
use ahma_core::utils::logging::init_test_logging;
use common::get_tools_dir;
use std::collections::HashSet;

/// Validates that all expected tools are present in the tools directory.
#[tokio::test]
async fn test_all_expected_tools_present() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let expected_tools = vec![
        "cargo",
        "file_tools",
        "gh",
        "git",
        "gradlew",
        "python",
        "sandboxed_shell",
    ];

    for tool_name in &expected_tools {
        assert!(
            configs.contains_key(*tool_name),
            "Expected tool '{}' not found in configs. Available: {:?}",
            tool_name,
            configs.keys().collect::<Vec<_>>()
        );
    }
}

/// Validates that all tool definitions have required fields.
#[tokio::test]
async fn test_all_tools_have_required_fields() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    for (name, config) in &configs {
        assert!(
            !config.name.is_empty(),
            "Tool '{}' must have a non-empty name",
            name
        );
        assert!(
            !config.description.is_empty(),
            "Tool '{}' must have a non-empty description",
            name
        );
        assert!(
            !config.command.is_empty(),
            "Tool '{}' must have a non-empty command",
            name
        );
    }
}

// ==================== cargo.json comprehensive tests ====================

#[tokio::test]
async fn test_cargo_tool_has_all_expected_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let cargo = configs.get("cargo").expect("Should find cargo tool");

    let expected_subcommands = vec![
        "build",
        "run",
        "add",
        "upgrade",
        "update",
        "check",
        "test",
        "fmt",
        "doc",
        "clippy",
        "qualitycheck",
        "audit",
    ];

    let subcommands = cargo
        .subcommand
        .as_ref()
        .expect("cargo should have subcommands");

    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "cargo tool missing expected subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_cargo_build_subcommand_options() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let cargo = configs.get("cargo").expect("Should find cargo tool");
    let build = find_subcommand(cargo, "build").expect("Should find build subcommand");

    // Check expected options exist
    let options = build.options.as_ref().expect("build should have options");
    let option_names: HashSet<_> = options.iter().map(|o| o.name.as_str()).collect();

    assert!(
        option_names.contains("release"),
        "cargo build should have 'release' option"
    );
    assert!(
        option_names.contains("workspace"),
        "cargo build should have 'workspace' option"
    );
}

#[tokio::test]
async fn test_cargo_clippy_subcommand_options() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let cargo = configs.get("cargo").expect("Should find cargo tool");
    let clippy = find_subcommand(cargo, "clippy").expect("Should find clippy subcommand");

    let options = clippy.options.as_ref().expect("clippy should have options");
    let option_names: HashSet<_> = options.iter().map(|o| o.name.as_str()).collect();

    assert!(
        option_names.contains("fix"),
        "cargo clippy should have 'fix' option"
    );
    assert!(
        option_names.contains("allow-dirty"),
        "cargo clippy should have 'allow-dirty' option"
    );
    assert!(
        option_names.contains("tests"),
        "cargo clippy should have 'tests' option"
    );
    assert!(
        option_names.contains("workspace"),
        "cargo clippy should have 'workspace' option"
    );

    // Verify force_synchronous is true (per R2.5 - must sync to avoid race conditions)
    assert_eq!(
        clippy.synchronous,
        Some(true),
        "cargo clippy should be force_synchronous=true"
    );
}

#[tokio::test]
async fn test_cargo_add_is_synchronous() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let cargo = configs.get("cargo").expect("Should find cargo tool");
    let add = find_subcommand(cargo, "add").expect("Should find add subcommand");

    // Per R2.5: Commands that modify Cargo.toml must be synchronous
    assert_eq!(
        add.synchronous,
        Some(true),
        "cargo add should be force_synchronous=true (modifies Cargo.toml)"
    );
}

#[tokio::test]
async fn test_cargo_nextest_run_exists() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let cargo = configs.get("cargo").expect("Should find cargo tool");
    let subcommands = cargo
        .subcommand
        .as_ref()
        .expect("cargo should have subcommands");

    // Find nextest subcommand which has nested 'run' subcommand
    let nextest = subcommands
        .iter()
        .find(|sc| sc.name == "nextest")
        .expect("cargo should have nextest subcommand");

    let nested = nextest
        .subcommand
        .as_ref()
        .expect("nextest should have nested subcommands");
    let run = nested
        .iter()
        .find(|sc| sc.name == "run")
        .expect("nextest should have 'run' subcommand");

    // Verify it has workspace option
    let options = run
        .options
        .as_ref()
        .expect("nextest run should have options");
    let has_workspace = options.iter().any(|o| o.name == "workspace");
    assert!(
        has_workspace,
        "cargo nextest run should have 'workspace' option"
    );
}

// ==================== file_tools.json comprehensive tests ====================

#[tokio::test]
async fn test_file_tools_has_all_expected_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let file_tools = configs
        .get("file_tools")
        .expect("Should find file_tools tool");

    let expected_subcommands = vec![
        "ls", "mv", "cp", "rm", "grep", "sed", "touch", "pwd", "cd", "cat", "find", "head", "tail",
        "diff",
    ];

    let subcommands = file_tools
        .subcommand
        .as_ref()
        .expect("file_tools should have subcommands");

    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "file_tools missing expected subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_file_tools_ls_options() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let file_tools = configs
        .get("file_tools")
        .expect("Should find file_tools tool");
    let ls = find_subcommand(file_tools, "ls").expect("Should find ls subcommand");

    let options = ls.options.as_ref().expect("ls should have options");
    let option_names: HashSet<_> = options.iter().map(|o| o.name.as_str()).collect();

    assert!(
        option_names.contains("long"),
        "ls should have 'long' option"
    );
    assert!(option_names.contains("all"), "ls should have 'all' option");
    assert!(
        option_names.contains("human-readable"),
        "ls should have 'human-readable' option"
    );
    assert!(
        option_names.contains("recursive"),
        "ls should have 'recursive' option"
    );

    // Verify force_synchronous for ls (should be true - quick operation)
    assert_eq!(
        ls.synchronous,
        Some(true),
        "file_tools ls should be force_synchronous=true"
    );
}

#[tokio::test]
async fn test_file_tools_grep_options() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let file_tools = configs
        .get("file_tools")
        .expect("Should find file_tools tool");
    let grep = find_subcommand(file_tools, "grep").expect("Should find grep subcommand");

    // Check positional args
    let pos_args = grep
        .positional_args
        .as_ref()
        .expect("grep should have positional_args");
    let pattern_arg = pos_args
        .iter()
        .find(|a| a.name == "pattern")
        .expect("grep should have 'pattern' positional arg");
    assert_eq!(
        pattern_arg.required,
        Some(true),
        "pattern should be required"
    );

    // Check options
    let options = grep.options.as_ref().expect("grep should have options");
    let option_names: HashSet<_> = options.iter().map(|o| o.name.as_str()).collect();

    assert!(
        option_names.contains("recursive"),
        "grep should have 'recursive' option"
    );
    assert!(
        option_names.contains("ignore-case"),
        "grep should have 'ignore-case' option"
    );
    assert!(
        option_names.contains("line-number"),
        "grep should have 'line-number' option"
    );
}

#[tokio::test]
async fn test_file_tools_all_subcommands_are_synchronous() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let file_tools = configs
        .get("file_tools")
        .expect("Should find file_tools tool");

    // Tool level force_synchronous should be true
    assert_eq!(
        file_tools.synchronous,
        Some(true),
        "file_tools should have force_synchronous=true at tool level"
    );

    // All subcommands should inherit or explicitly set force_synchronous=true
    let subcommands = file_tools
        .subcommand
        .as_ref()
        .expect("file_tools should have subcommands");

    for sc in subcommands {
        let effective_sync = sc.synchronous.or(file_tools.synchronous).unwrap_or(false);
        assert!(
            effective_sync,
            "file_tools subcommand '{}' should be effectively synchronous",
            sc.name
        );
    }
}

// ==================== git.json comprehensive tests ====================

#[tokio::test]
async fn test_git_tool_has_all_expected_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let git = configs.get("git").expect("Should find git tool");

    let expected_subcommands = vec!["status", "add", "commit", "push", "log"];

    let subcommands = git
        .subcommand
        .as_ref()
        .expect("git should have subcommands");

    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "git tool missing expected subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_git_status_is_synchronous() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let git = configs.get("git").expect("Should find git tool");
    let status = find_subcommand(git, "status").expect("Should find status subcommand");

    assert_eq!(
        status.synchronous,
        Some(true),
        "git status should be force_synchronous=true (quick status check)"
    );
}

#[tokio::test]
async fn test_git_commit_has_message_option_with_file_arg() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let git = configs.get("git").expect("Should find git tool");
    let commit = find_subcommand(git, "commit").expect("Should find commit subcommand");

    let options = commit.options.as_ref().expect("commit should have options");
    let message_opt = options
        .iter()
        .find(|o| o.name == "message")
        .expect("commit should have 'message' option");

    // Message should use file_arg for multi-line commit messages
    assert_eq!(
        message_opt.file_arg,
        Some(true),
        "git commit message should have file_arg=true for multi-line support"
    );
    assert_eq!(
        message_opt.file_flag.as_deref(),
        Some("-F"),
        "git commit message should use -F flag for file input"
    );
}

// ==================== gh.json comprehensive tests ====================

#[tokio::test]
async fn test_gh_tool_pr_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let gh = configs.get("gh").expect("Should find gh tool");

    let expected_pr_subcommands = vec!["pr_create", "pr_list", "pr_view", "pr_close"];

    let subcommands = gh.subcommand.as_ref().expect("gh should have subcommands");
    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_pr_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "gh tool missing expected PR subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_gh_tool_run_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let gh = configs.get("gh").expect("Should find gh tool");

    let expected_run_subcommands = vec![
        "run_cancel",
        "run_download",
        "run_list",
        "run_view",
        "run_watch",
    ];

    let subcommands = gh.subcommand.as_ref().expect("gh should have subcommands");
    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_run_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "gh tool missing expected run subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_gh_tool_workflow_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let gh = configs.get("gh").expect("Should find gh tool");

    let expected_workflow_subcommands = vec!["workflow_view", "workflow_list"];

    let subcommands = gh.subcommand.as_ref().expect("gh should have subcommands");
    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_workflow_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "gh tool missing expected workflow subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_gh_is_fully_synchronous() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let gh = configs.get("gh").expect("Should find gh tool");

    // gh tool should be synchronous at tool level
    assert_eq!(
        gh.synchronous,
        Some(true),
        "gh tool should be force_synchronous=true at tool level"
    );
}

// ==================== gradlew.json comprehensive tests ====================

#[tokio::test]
async fn test_gradlew_has_sync_and_async_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let gradlew = configs.get("gradlew").expect("Should find gradlew tool");
    let subcommands = gradlew
        .subcommand
        .as_ref()
        .expect("gradlew should have subcommands");

    // These should be synchronous (quick operations)
    let sync_subcommands = vec!["tasks", "help", "dependencies", "properties", "clean"];

    // These should be asynchronous (long-running)
    let async_subcommands = vec!["build", "assemble", "test", "lint"];

    for sync_name in &sync_subcommands {
        if let Some(sc) = subcommands.iter().find(|s| s.name == *sync_name) {
            assert_eq!(
                sc.synchronous,
                Some(true),
                "gradlew {} should be force_synchronous=true",
                sync_name
            );
        }
    }

    for async_name in &async_subcommands {
        if let Some(sc) = subcommands.iter().find(|s| s.name == *async_name) {
            assert_eq!(
                sc.synchronous,
                Some(false),
                "gradlew {} should be force_synchronous=false",
                async_name
            );
        }
    }
}

// ==================== sandboxed_shell.json comprehensive tests ====================

#[tokio::test]
async fn test_sandboxed_shell_has_command_positional_arg() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let sandboxed_shell = configs
        .get("sandboxed_shell")
        .expect("Should find sandboxed_shell tool");

    // sandboxed_shell should have default subcommand with command positional arg
    let subcommands = sandboxed_shell
        .subcommand
        .as_ref()
        .expect("sandboxed_shell should have subcommands");

    let default_sc = subcommands
        .iter()
        .find(|sc| sc.name == "default")
        .expect("sandboxed_shell should have 'default' subcommand");

    let pos_args = default_sc
        .positional_args
        .as_ref()
        .expect("default subcommand should have positional_args");

    let command_arg = pos_args
        .iter()
        .find(|a| a.name == "command")
        .expect("default subcommand should have 'command' positional arg");

    assert_eq!(
        command_arg.required,
        Some(true),
        "command arg should be required"
    );
}

#[tokio::test]
async fn test_sandboxed_shell_has_working_directory_option() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let sandboxed_shell = configs
        .get("sandboxed_shell")
        .expect("Should find sandboxed_shell tool");

    let subcommands = sandboxed_shell
        .subcommand
        .as_ref()
        .expect("sandboxed_shell should have subcommands");

    let default_sc = subcommands
        .iter()
        .find(|sc| sc.name == "default")
        .expect("sandboxed_shell should have 'default' subcommand");

    let options = default_sc
        .options
        .as_ref()
        .expect("default subcommand should have options");

    let wd_opt = options
        .iter()
        .find(|o| o.name == "working_directory")
        .expect("default subcommand should have 'working_directory' option");

    assert_eq!(
        wd_opt.format.as_deref(),
        Some("path"),
        "working_directory should have format=path for security validation"
    );
}

// ==================== python.json comprehensive tests ====================

#[tokio::test]
async fn test_python_tool_has_all_expected_subcommands() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let python = configs.get("python").expect("Should find python tool");

    let expected_subcommands = vec![
        "script",
        "code",
        "module",
        "version",
        "help",
        "interactive",
        "check",
    ];

    let subcommands = python
        .subcommand
        .as_ref()
        .expect("python should have subcommands");

    let subcommand_names: HashSet<_> = subcommands.iter().map(|sc| sc.name.as_str()).collect();

    for expected in &expected_subcommands {
        assert!(
            subcommand_names.contains(expected),
            "python tool missing expected subcommand: '{}'. Found: {:?}",
            expected,
            subcommand_names
        );
    }
}

#[tokio::test]
async fn test_python_script_subcommand_has_file_option() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let python = configs.get("python").expect("Should find python tool");
    let script = find_subcommand(python, "script").expect("Should find script subcommand");

    let options = script.options.as_ref().expect("script should have options");
    let file_opt = options
        .iter()
        .find(|o| o.name == "file")
        .expect("script should have 'file' option");

    assert_eq!(
        file_opt.format.as_deref(),
        Some("path"),
        "python script file option should have format=path"
    );
    assert_eq!(
        file_opt.required,
        Some(true),
        "python script file option should be required"
    );
}

#[tokio::test]
async fn test_python_code_subcommand_has_command_option() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let python = configs.get("python").expect("Should find python tool");
    let code = find_subcommand(python, "code").expect("Should find code subcommand");

    let options = code.options.as_ref().expect("code should have options");
    let cmd_opt = options
        .iter()
        .find(|o| o.name == "command")
        .expect("code should have 'command' option");

    assert_eq!(
        cmd_opt.required,
        Some(true),
        "python code command option should be required"
    );
}

#[tokio::test]
async fn test_python_module_is_async() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let python = configs.get("python").expect("Should find python tool");
    let module = find_subcommand(python, "module").expect("Should find module subcommand");

    // module subcommand should be async (long-running potentially)
    assert_eq!(
        module.synchronous,
        Some(false),
        "python module should be force_synchronous=false"
    );
}

#[tokio::test]
async fn test_python_version_is_sync() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir)
        .await
        .expect("Should load tools config");

    let python = configs.get("python").expect("Should find python tool");

    // Tool level should be synchronous
    assert_eq!(
        python.synchronous,
        Some(true),
        "python tool should be force_synchronous=true at tool level"
    );

    // version subcommand should inherit or be sync
    let version = find_subcommand(python, "version").expect("Should find version subcommand");
    let effective_sync = version.synchronous.or(python.synchronous).unwrap_or(false);
    assert!(
        effective_sync,
        "python version should be effectively synchronous"
    );
}

// ==================== Helper functions ====================

fn find_subcommand<'a>(tool: &'a ToolConfig, name: &str) -> Option<&'a SubcommandConfig> {
    tool.subcommand
        .as_ref()
        .and_then(|scs| scs.iter().find(|sc| sc.name == name))
}
