use crate::model::ProjectEnvironment;
use anyhow::Context;
use std::ffi::OsStr;
use std::io::Write;
use std::process::{exit, Stdio};
use tempfile::NamedTempFile;
use tokio::process::Command;

impl ProjectEnvironment {
    pub fn run_command(&self, prog: impl AsRef<OsStr>, apply_user: bool) -> Command {
        let mut process = Command::new(prog);
        for (name, value) in &self.environ {
            let value = format!("{value}:{}", std::env::var(name).unwrap_or_default());
            process.env(name, &value);
        }

        if apply_user {
            for (name, value) in &self.user_environ {
                let value = format!("{value}:{}", std::env::var(name).unwrap_or_default());
                process.env(name, &value);
            }
        }
        process
    }

    pub async fn run_shell(&self) -> anyhow::Result<()> {
        // Write init script to temp
        let status = if let Some(shell_hook) = &self.shell_hook {
            let mut file = NamedTempFile::new().context("creating init script file")?;
            file.write_all(shell_hook.as_bytes())
                .context("writing script file")?;
            let _ = file.flush();
            let (_, file_path) = file.keep().context("keeping temporary file")?;

            println!("Using init file {}", file_path.display());

            self.run_command("bash", true)
                .arg("--rcfile")
                .arg(file_path.to_str().unwrap_or_default())
                .arg("-i")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .context("spawning shell")?
                .wait()
                .await
                .context("wait for output")?
        } else {
            self.run_command("bash", true)
                .arg("-i")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .context("spawning shell")?
                .wait()
                .await
                .context("wait for output")?
        };

        exit(status.code().unwrap_or_default());
    }
}
