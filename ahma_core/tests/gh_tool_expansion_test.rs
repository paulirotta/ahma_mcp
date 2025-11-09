mod common;

use ahma_core::config::load_tool_configs;
use ahma_core::utils::logging::init_test_logging;
use common::get_tools_dir;

#[tokio::test]
async fn test_gh_tool_expansion_all_synchronous() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir).expect("Should load tools config");

    // Find the gh tool
    let gh_tool = configs.get("gh").expect("Should find gh tool in config");

    // Verify all expected subcommands are present and synchronous
    let expected_subcommands = vec![
        "pr_create",
        "pr_list",
        "pr_view",
        "pr_close",
        "cache_list",
        "cache_delete",
        "run_cancel",
        "run_download",
        "run_list",
        "run_view",
        "run_watch",
        "workflow_view",
        "workflow_list",
    ];

    let subcommands = gh_tool
        .subcommand
        .as_ref()
        .expect("Should have subcommands");
    assert_eq!(subcommands.len(), 13, "Should have exactly 13 subcommands");

    // Check that tool has synchronous = true at tool level for inheritance
    assert_eq!(
        gh_tool.synchronous,
        Some(true),
        "Tool should have synchronous=true for subcommand inheritance"
    );

    for expected_name in &expected_subcommands {
        let subcommand = subcommands
            .iter()
            .find(|sc| sc.name == *expected_name)
            .unwrap_or_else(|| panic!("Should find subcommand {}", expected_name));

        // With inheritance, subcommands should have None and inherit from tool level
        assert!(
            subcommand
                .synchronous
                .or(gh_tool.synchronous)
                .unwrap_or(false),
            "Subcommand {} should be synchronous",
            expected_name
        );

        assert!(
            !subcommand.description.is_empty(),
            "Subcommand {} should have a description",
            expected_name
        );
    }
}

#[tokio::test]
async fn test_gh_cache_subcommands_schema() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir).expect("Should load tools config");
    let gh_tool = configs.get("gh").expect("Should find gh tool");

    let subcommands = gh_tool
        .subcommand
        .as_ref()
        .expect("Should have subcommands");

    // Test cache_list options
    let cache_list = subcommands
        .iter()
        .find(|sc| sc.name == "cache_list")
        .expect("Should find cache_list subcommand");

    let expected_options = vec!["repo", "key", "limit", "order", "ref", "sort"];
    assert_eq!(
        cache_list.options.as_ref().map_or(0, |opts| opts.len()),
        expected_options.len()
    );

    if let Some(options) = &cache_list.options {
        for expected_opt in expected_options {
            let option = options
                .iter()
                .find(|opt| opt.name == expected_opt)
                .unwrap_or_else(|| panic!("Should find option {}", expected_opt));
            assert!(
                option.description.is_some() && !option.description.as_ref().unwrap().is_empty()
            );
        }
    }

    // Test cache_delete has positional arg and options
    let cache_delete = subcommands
        .iter()
        .find(|sc| sc.name == "cache_delete")
        .expect("Should find cache_delete subcommand");

    assert_eq!(
        cache_delete
            .positional_args
            .as_ref()
            .map_or(0, |args| args.len()),
        1
    );
    if let Some(pos_args) = &cache_delete.positional_args {
        assert_eq!(pos_args[0].name, "cache_id_or_key");
        assert_eq!(pos_args[0].required, Some(false));
    }

    // Should have repo, all, and succeed-on-no-caches options
    if let Some(options) = &cache_delete.options {
        assert!(options.iter().any(|opt| opt.name == "all"));
        assert!(options.iter().any(|opt| opt.name == "succeed-on-no-caches"));
    }
}

#[tokio::test]
async fn test_gh_run_subcommands_schema() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir).expect("Should load tools config");
    let gh_tool = configs.get("gh").expect("Should find gh tool");

    let subcommands = gh_tool
        .subcommand
        .as_ref()
        .expect("Should have subcommands");

    // Test run subcommands that require run_id
    let required_run_id_commands = vec!["run_cancel", "run_download", "run_watch"];

    for cmd_name in required_run_id_commands {
        let subcommand = subcommands
            .iter()
            .find(|sc| sc.name == cmd_name)
            .unwrap_or_else(|| panic!("Should find {} subcommand", cmd_name));

        assert_eq!(
            subcommand
                .positional_args
                .as_ref()
                .map_or(0, |args| args.len()),
            1
        );
        if let Some(pos_args) = &subcommand.positional_args {
            assert_eq!(pos_args[0].name, "run_id");
            assert_eq!(pos_args[0].required, Some(true));
        }

        // Should have minimal options (just repo)
        assert!(subcommand.options.as_ref().map_or(0, |opts| opts.len()) <= 1);
        if let Some(options) = &subcommand.options {
            if !options.is_empty() {
                assert_eq!(options[0].name, "repo");
            }
        }
    }

    // Test run_view has optional run_id
    let run_view = subcommands
        .iter()
        .find(|sc| sc.name == "run_view")
        .expect("Should find run_view subcommand");

    assert_eq!(
        run_view
            .positional_args
            .as_ref()
            .map_or(0, |args| args.len()),
        1
    );
    if let Some(pos_args) = &run_view.positional_args {
        assert_eq!(pos_args[0].required, Some(false));
    }

    // Test run_list has no positional args
    let run_list = subcommands
        .iter()
        .find(|sc| sc.name == "run_list")
        .expect("Should find run_list subcommand");

    assert_eq!(
        run_list
            .positional_args
            .as_ref()
            .map_or(0, |args| args.len()),
        0
    );
}

#[tokio::test]
async fn test_gh_workflow_subcommands_schema() {
    init_test_logging();
    let tools_dir = get_tools_dir();
    let configs = load_tool_configs(&tools_dir).expect("Should load tools config");
    let gh_tool = configs.get("gh").expect("Should find gh tool");

    let subcommands = gh_tool
        .subcommand
        .as_ref()
        .expect("Should have subcommands");

    // Test workflow_view has optional workflow_selector
    let workflow_view = subcommands
        .iter()
        .find(|sc| sc.name == "workflow_view")
        .expect("Should find workflow_view subcommand");

    assert_eq!(
        workflow_view
            .positional_args
            .as_ref()
            .map_or(0, |args| args.len()),
        1
    );
    if let Some(pos_args) = &workflow_view.positional_args {
        assert_eq!(pos_args[0].name, "workflow_selector");
        assert_eq!(pos_args[0].required, Some(false));
    }

    // Test workflow_list has no positional args
    let workflow_list = subcommands
        .iter()
        .find(|sc| sc.name == "workflow_list")
        .expect("Should find workflow_list subcommand");

    assert_eq!(
        workflow_list
            .positional_args
            .as_ref()
            .map_or(0, |args| args.len()),
        0
    );
}
