// mod deps;
use std::fs::{File, Permissions};
use std::io::{Read, Write};
use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::model::ProjectDesc;
use crate::run_as::run_as;
use anyhow::Context;
use clap::{Parser, Subcommand};
use exec::execvp;
use tempfile::NamedTempFile;

mod install;
mod model;
mod run_as;
mod ser;
mod shell;

async fn run() -> anyhow::Result<()> {
    todo!()
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[clap(short, default_value = "homebrewer.toml")]
    /// Use this TOML
    path_to_toml: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    Shell,
}

fn main() -> anyhow::Result<()> {
    let Cli {
        command,
        path_to_toml,
    } = Cli::parse();

    match command {
        Commands::Shell => {
            let mut file = File::open(&path_to_toml).context("Opening project file")?;
            let mut file_contents = Default::default();
            file.read_to_string(&mut file_contents)
                .context("Reading file contents")?;
            let project: ProjectDesc =
                toml::from_str(&file_contents).context("Parsing toml file")?;

            // println!("Got {project:#?}");

            let project_dir = path_to_toml.parent().context("Getting parent")?;

            let info = project
                .to_environment(
                    project_dir.to_str().context("path to dir")?,
                    project_dir.join(".hb-state").to_str().unwrap_or_default(),
                )
                .expect("environment");

            // println!("Got {info:#?}");
            info.apply_current()
                .context("applying current environment")?;

            // Write init script to temp
            let err = if let Some(shell_hook) = &info.shell_hook {
                let mut file = NamedTempFile::new().context("creating init script file")?;
                file.write_all(shell_hook.as_bytes())
                    .context("writing script file")?;
                let _ = file.flush();
                file.as_file()
                    .set_permissions(Permissions::from_mode(777))
                    .context("setting init script permission")?;

                let file_path = file.path().to_str().unwrap_or_default();
                println!("Writing to {file_path}");

                execvp("bash", &["--login", "--rcfile", file_path])
            } else {
                execvp("bash", &["--login"])
            };

            println!("{}", err);
        }
    }

    Ok(())
}
