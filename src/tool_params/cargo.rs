use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoAddParams {
    pub package: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoAuditParams {
    #[serde(default)]
    pub json: bool,
    #[serde(default)]
    pub fix: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoBenchParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub lib: bool,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub bins: bool,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub examples: bool,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub tests: bool,
    #[serde(default)]
    pub bench: Option<String>,
    #[serde(default)]
    pub benches: bool,
    #[serde(default, alias = "all-targets")]
    pub all_targets: bool,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, alias = "no-run")]
    pub no_run: bool,
    #[serde(default, alias = "no-fail-fast")]
    pub no_fail_fast: bool,
    #[serde(default)]
    pub message_format: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub benchname: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoBuildParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub lib: bool,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub bins: bool,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub examples: bool,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub tests: bool,
    #[serde(default)]
    pub bench: Option<String>,
    #[serde(default)]
    pub benches: bool,
    #[serde(default, alias = "all-targets")]
    pub all_targets: bool,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub message_format: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoCheckParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub lib: bool,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub bins: bool,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub examples: bool,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub tests: bool,
    #[serde(default)]
    pub bench: Option<String>,
    #[serde(default)]
    pub benches: bool,
    #[serde(default, alias = "all-targets")]
    pub all_targets: bool,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub message_format: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoCleanParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, alias = "doc-only")]
    pub doc: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoClippyParams {
    #[serde(default)]
    pub fix: bool,
    #[serde(default, alias = "allow-dirty")]
    pub allow_dirty: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoDocParams {
    #[serde(default)]
    pub open: bool,
    #[serde(default, alias = "no-deps")]
    pub no_deps: bool,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub lib: bool,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub bins: bool,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub message_format: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoFetchParams {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoFmtParams {
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub check: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoInstallParams {
    pub package: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub force: bool,
    #[serde(default, alias = "no-track")]
    pub no_track: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoMetadataParams {
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default, alias = "no-deps")]
    pub no_deps: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub format_version: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoNextestParams {
    #[serde(default)]
    pub workspace: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoRemoveParams {
    pub package: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoRunParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub message_format: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoSearchParams {
    pub query: String,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoTestParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub lib: bool,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub bins: bool,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub examples: bool,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub tests: bool,
    #[serde(default)]
    pub bench: Option<String>,
    #[serde(default)]
    pub benches: bool,
    #[serde(default, alias = "all-targets")]
    pub all_targets: bool,
    #[serde(default)]
    pub doc: bool,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "target-dir")]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, alias = "no-run")]
    pub no_run: bool,
    #[serde(default, alias = "no-fail-fast")]
    pub no_fail_fast: bool,
    #[serde(default)]
    pub message_format: Option<String>,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub testname: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoTreeParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub features: Option<String>,
    #[serde(default, alias = "all-features")]
    pub all_features: bool,
    #[serde(default, alias = "no-default-features")]
    pub no_default_features: bool,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub no_dedupe: bool,
    #[serde(default)]
    pub duplicates: bool,
    #[serde(default)]
    pub edge: Option<String>,
    #[serde(default)]
    pub charset: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub prefix_depth: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoUpdateParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub aggressive: bool,
    #[serde(default)]
    pub precise: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CargoUpgradeParams {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub workspace: bool,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub dev: bool,
    #[serde(default)]
    pub build: bool,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub offline: bool,
    #[serde(default)]
    pub manifest_path: Option<String>,
}
