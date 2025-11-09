use super::{get_workspace_dir, get_workspace_path};
use anyhow::Result;
use rmcp::{
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use std::path::Path;
use tokio::process::Command;

#[allow(dead_code)]
pub async fn new_client(tools_dir: Option<&str>) -> Result<RunningService<RoleClient, ()>> {
    new_client_with_args(tools_dir, &[]).await
}

#[allow(dead_code)]
pub async fn new_client_with_args(
    tools_dir: Option<&str>,
    extra_args: &[&str],
) -> Result<RunningService<RoleClient, ()>> {
    let workspace_dir = get_workspace_dir();
    let client = ()
        .serve(TokioChildProcess::new(Command::new("cargo").configure(
            |cmd| {
                cmd.current_dir(&workspace_dir)
                    .arg("run")
                    .arg("--package")
                    .arg("ahma_shell")
                    .arg("--bin")
                    .arg("ahma_mcp")
                    .arg("--");
                if let Some(dir) = tools_dir {
                    let tools_path = if Path::new(dir).is_absolute() {
                        Path::new(dir).to_path_buf()
                    } else {
                        get_workspace_path(dir)
                    };
                    cmd.arg("--tools-dir").arg(tools_path);
                }
                for arg in extra_args {
                    cmd.arg(arg);
                }
            },
        ))?)
        .await?;
    Ok(client)
}
