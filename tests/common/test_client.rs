use anyhow::Result;
use rmcp::{
    ServiceExt,
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use tokio::process::Command;

#[allow(dead_code)]
pub async fn new_client(tools_dir: Option<&str>) -> Result<RunningService<RoleClient, ()>> {
    let client = ()
        .serve(TokioChildProcess::new(Command::new("cargo").configure(
            |cmd| {
                cmd.arg("run").arg("--bin").arg("ahma_mcp").arg("--");
                if let Some(dir) = tools_dir {
                    cmd.arg("--tools-dir").arg(dir);
                }
            },
        ))?)
        .await?;
    Ok(client)
}
