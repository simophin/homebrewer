use crate::model::ProjectEnvironment;
use anyhow::{bail, Context};
use std::process::Stdio;

impl ProjectEnvironment {
    pub async fn run_script(&self, name: &str) -> anyhow::Result<()> {
        let script = self
            .scripts
            .get(name)
            .with_context(|| format!("unable to find script named '{name}'"))?;

        let status = self
            .run_command("sh", true)
            .arg("-c")
            .arg(script)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawning script")?
            .wait()
            .await
            .context("waiting for script")?;

        if !status.success() {
            bail!("Unable to run script")
        }

        Ok(())
    }
}
