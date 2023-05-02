use anyhow::Context;

use crate::model::ProjectEnvironment;

pub async fn print_direnv_commands(project: &ProjectEnvironment) -> anyhow::Result<()> {
    project
        .run_command("direnv", true)
        .arg("dump")
        .spawn()?
        .wait()
        .await
        .context("waiting for child")?;
    Ok(())
}
