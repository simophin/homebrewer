use crate::model::ProjectEnvironment;
use anyhow::Context;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::io::Write;
use std::process::{exit, Stdio};
use tempfile::NamedTempFile;
use tokio::process::Command;

impl ProjectEnvironment {
    pub fn run_command(&self, prog: impl AsRef<OsStr>, apply_user: bool) -> Command {
        let mut process = Command::new(prog);
        for (name, value) in &self.environ {
            let existing = std::env::var(name).unwrap_or_default();
            if existing.is_empty() {
                process.env(name, &value);
            } else {
                process.env(name, format!("{value}:{existing}"));
            }
        }

        if apply_user {
            for (name, value) in &self.user_environ {
                process.env(name, &value);
            }
        }
        process
    }

    pub async fn run_shell(&self, command: Option<impl AsRef<str> + Debug>) -> anyhow::Result<()> {
        let mut cmd = self.run_command("bash", true);

        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = if let Some(command) = command {
            let command = format!(
                "set -e\n {}\n {}",
                self.shell_hook
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or_default(),
                command.as_ref()
            );

            cmd.arg("-c")
                .arg(command)
                .spawn()
                .context("spawning shell")?
                .wait()
                .await
                .context("wait for output")?
        } else if let Some(hook) = self.shell_hook.as_ref() {
            let file = NamedTempFile::new().context("Creating temp file")?;
            let (mut file, path) = file.keep().context("keeping tempfile")?;
            file.write_fmt(format_args!("rm -f {}\n", path.display()))
                .context("writing tempfile")?;
            file.write_all(hook.as_bytes()).context("writing hook")?;
            drop(file);

            // Running as an interactive shell
            let mut child = cmd
                .arg("--rcfile")
                .arg(path)
                .spawn()
                .context("spawning shell")?;

            child.wait().await.context("wait for output")?
        } else {
            cmd.spawn()
                .context("spawning shell")?
                .wait()
                .await
                .context("wait for output")?
        };

        exit(status.code().unwrap_or_default());
    }
}
