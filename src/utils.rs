use std::{ffi::OsStr, process::Stdio};

use anyhow::{bail, Context};
use tokio::process::Command;

pub async fn gather_command_output(cmd: &mut Command) -> anyhow::Result<String> {
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Error spawning command")?
        .wait_with_output()
        .await
        .context("Error waiting for output")?;

    if !output.status.success() {
        bail!(
            "Error running command: \n{}",
            std::str::from_utf8(&output.stderr).unwrap_or_default()
        );
    }

    String::from_utf8(output.stdout).context("converting output to string")
}

pub async fn brew_prefixes(
    desc: impl Iterator<Item = impl AsRef<OsStr>>,
) -> anyhow::Result<Vec<String>> {
    Ok(
        gather_command_output(Command::new("brew").arg("--prefix").args(desc))
            .await?
            .trim()
            .lines()
            .map(|s| s.to_string())
            .collect(),
    )
}
