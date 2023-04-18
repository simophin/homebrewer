use crate::model::ProjectEnvironment;
use anyhow::Context;
use nix::libc::{kill, pid_t, SIGTERM};
use std::os::unix::prelude::ExitStatusExt;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::select;
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

impl ProjectEnvironment {
    pub async fn run_services(&self) -> anyhow::Result<()> {
        let cancel_token = CancellationToken::new();
        let mut js = JoinSet::new();

        for (name, _) in &self.services {
            js.spawn(self.clone().run_service(name.clone(), cancel_token.clone()));
        }

        select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down services");
            },
            _ = js.join_next() => {}
        }

        cancel_token.cancel();

        while js.join_next().await.is_some() {}

        Ok(())
    }

    pub async fn run_service(
        self,
        name: String,
        cancellation: CancellationToken,
    ) -> anyhow::Result<ExitStatus> {
        let service = self
            .services
            .get(&name)
            .with_context(|| format!("Unable to find service {name}"))?;

        std::fs::create_dir_all(&service.working_directory)
            .with_context(|| format!("Error creating state directory for service {name}"))?;

        let mut cmd = self
            .run_command("sh", false)
            .arg("-c")
            .arg(&service.script)
            .current_dir(&service.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Spawning service")?;

        let mut stdout = BufReader::new(cmd.stdout.take().context("taking out stdout")?).lines();
        let mut stderr = BufReader::new(cmd.stderr.take().context("taking out stderr")?).lines();
        let pid: pid_t = cmd
            .id()
            .context("getting child process id")?
            .try_into()
            .context("converting to pid")?;

        println!("Running service {name}");
        let mut error = None;

        loop {
            select! {
                _ = cancellation.cancelled() => {
                    break;
                }

                _ = cmd.wait() => {
                    break;
                }

                line = stdout.next_line() => {
                    match line.context("reading line") {
                        Ok(Some(line)) => {
                            println!("{name}: {line}");
                        }

                        Ok(None) => continue,
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    }
                }

                line = stderr.next_line() => {
                    match line.context("reading error line") {
                        Ok(Some(line)) => {
                            eprintln!("{name}: {line}");
                        }

                        Ok(None) => continue,
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(status) = cmd.try_wait().context("trying to wait for child")? {
            return Ok(status);
        }

        println!("Gracefully waiting for {name} to terminate");
        unsafe {
            kill(pid, SIGTERM);
        }

        let timeout_duration = Duration::from_secs(5);
        match timeout(timeout_duration, cmd.wait()).await {
            Err(_) => {
                eprintln!("{name} doesn't respond within {timeout_duration:?}, killing...");
                let _ = cmd.kill();
            }

            Ok(status) => {
                println!("{name} exited with status {status:?}");
                return status.context("waiting for termination");
            }
        }

        if let Some(err) = error {
            return Err(err);
        }

        Ok(ExitStatus::from_raw(0))
    }
}
