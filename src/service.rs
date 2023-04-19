use crate::model::ProjectEnvironment;
use anyhow::{bail, Context};
use nix::libc::{kill, pid_t, SIGTERM};
use std::os::unix::prelude::ExitStatusExt;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio::{select, spawn};
use tokio_util::sync::CancellationToken;

impl ProjectEnvironment {
    pub async fn run_services(&self, only: Option<Vec<String>>) -> anyhow::Result<()> {
        let cancel_token = CancellationToken::new();
        let mut js = JoinSet::new();

        if let Some(only) = only {
            if let Some(service) = only
                .iter()
                .filter(|service| !self.services.contains_key(service.as_str()))
                .next()
            {
                bail!("Service {service} does not exist")
            }

            for name in only {
                js.spawn(self.clone().run_service(name.clone(), cancel_token.clone()));
            }
        } else {
            for (name, _) in &self.services {
                js.spawn(self.clone().run_service(name.clone(), cancel_token.clone()));
            }
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

    async fn monitor_outputs(
        name: String,
        stdout: impl AsyncBufRead + Unpin + 'static,
        stderr: impl AsyncBufRead + Unpin + 'static,
    ) {
        let mut stdout = stdout.lines();
        let mut stderr = stderr.lines();

        loop {
            select! {
                line = stdout.next_line() => {
                    if let Ok(Some(line)) = line {
                        println!("{name}: {line}");
                    }
                }

                line = stderr.next_line() => {
                    if let Ok(Some(line)) = line {
                        eprintln!("{name}: {line}");
                    }
                }
            }
        }
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

        let mut child = self
            .run_command("sh", false)
            .arg("-c")
            .arg(&service.script)
            .current_dir(&service.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Spawning service")?;

        let stdout = BufReader::new(child.stdout.take().context("taking out stdout")?);
        let stderr = BufReader::new(child.stderr.take().context("taking out stderr")?);
        let log_monitor = spawn(Self::monitor_outputs(name.clone(), stdout, stderr));

        let pid: pid_t = child
            .id()
            .context("getting child process id")?
            .try_into()
            .context("converting to pid")?;

        println!("Running service {name}");

        let status: Option<ExitStatus> = select! {
            _ = cancellation.cancelled() => None,
            status = child.wait() => status.ok(),
        };

        if status.is_none() {
            println!("Terminating {name}");
            unsafe {
                kill(pid, SIGTERM);
            }
        }

        println!("Gracefully waiting for {name} to terminate");

        let timeout_duration = Duration::from_secs(10);
        match timeout(timeout_duration, child.wait()).await {
            Err(_) => {
                eprintln!("{name} doesn't respond within {timeout_duration:?}, killing...");
                let _ = child.kill();
                let _ = log_monitor.abort();
            }

            Ok(status) => {
                println!("{name} exited with status {status:?}");
                let _ = log_monitor.abort();
                return status.context("waiting for termination");
            }
        }

        Ok(ExitStatus::from_raw(0))
    }
}
