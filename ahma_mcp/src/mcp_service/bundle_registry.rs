//! # Bundle Registry
//!
//! Maps CLI bundle flags to tool config names. This is the single source of truth
//! for progressive disclosure: when a user calls `discover_tools reveal <bundle>`,
//! the registry determines which tool configs to expose.

use std::collections::HashSet;

/// Metadata for a single tool bundle.
#[derive(Debug, Clone)]
pub struct BundleInfo {
    /// Human-facing bundle name (matches CLI flag minus `--`).
    pub name: &'static str,
    /// The `ToolConfig.name` value produced by the bundle's JSON.
    pub config_tool_name: &'static str,
    /// Short description shown by `discover_tools list`.
    pub description: &'static str,
}

/// All known bundles. Order determines listing order.
pub const BUNDLES: &[BundleInfo] = &[
    BundleInfo {
        name: "rust",
        config_tool_name: "cargo",
        description: "Cargo/Rust toolchain — build, test, clippy, fmt, doc, audit, nextest",
    },
    BundleInfo {
        name: "fileutils",
        config_tool_name: "file-tools",
        description: "Unix file operations — ls, cp, mv, rm, grep, sed, find, diff",
    },
    BundleInfo {
        name: "github",
        config_tool_name: "gh",
        description: "GitHub CLI — pull requests, Actions, caches, workflows",
    },
    BundleInfo {
        name: "git",
        config_tool_name: "git",
        description: "Git version control — status, add, commit, push, log",
    },
    BundleInfo {
        name: "gradle",
        config_tool_name: "gradlew",
        description: "Android Gradle wrapper — build, test, lint, assemble, install",
    },
    BundleInfo {
        name: "python",
        config_tool_name: "python",
        description: "Python interpreter — scripts, inline code, modules",
    },
    BundleInfo {
        name: "simplify",
        config_tool_name: "simplify",
        description: "Code complexity analyzer — reports hotspots with AI fix suggestions",
    },
];

/// Returns the set of `config_tool_name` values for bundles that are loaded
/// (i.e., their configs are present in the config map).
pub fn loaded_bundle_names(config_keys: &HashSet<String>) -> Vec<&'static BundleInfo> {
    BUNDLES
        .iter()
        .filter(|b| config_keys.contains(b.config_tool_name))
        .collect()
}

/// Looks up a bundle by its human-facing name.
pub fn find_bundle(name: &str) -> Option<&'static BundleInfo> {
    BUNDLES.iter().find(|b| b.name == name)
}

/// Returns the config tool name for a bundle by its human-facing name.
pub fn bundle_config_name(name: &str) -> Option<&'static str> {
    find_bundle(name).map(|b| b.config_tool_name)
}
