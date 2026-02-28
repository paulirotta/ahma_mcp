mod conversion;
mod exclusion;
pub mod paths;
mod pipeline;
pub mod workspace;

pub use paths::{get_package_name, get_relative_path};
pub use pipeline::{perform_analysis, run_analysis};
pub use workspace::{get_project_name, is_cargo_workspace};
