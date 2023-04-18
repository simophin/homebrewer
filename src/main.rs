use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::model::{ProjectDesc, ProjectEnvironment};
use anyhow::Context;
use clap::{Parser, Subcommand};

mod model;
mod run;
mod ser;
mod service;
mod shell;

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
    Up,
    Run { script_name: String },
}

fn read_project(toml_file: impl AsRef<Path>) -> anyhow::Result<ProjectEnvironment> {
    let toml_file = if toml_file.as_ref().is_relative() {
        std::env::current_dir()
            .context("getting current dir")?
            .join(toml_file)
    } else {
        toml_file.as_ref().to_path_buf()
    };

    let mut file = File::open(&toml_file).context("Opening project file")?;
    let mut file_contents = Default::default();
    file.read_to_string(&mut file_contents)
        .context("Reading file contents")?;
    let project: ProjectDesc = toml::from_str(&file_contents).context("Parsing toml file")?;

    let project_dir = toml_file.parent().context("Getting parent")?;

    project
        .to_environment(
            project_dir.to_str().context("path to dir")?,
            project_dir.join(".hb-state").to_str().unwrap_or_default(),
        )
        .context("environment")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        command,
        path_to_toml,
    } = Cli::parse();

    let info = read_project(&path_to_toml).context("reading project file")?;

    match command {
        Commands::Shell => info.run_shell().await,

        Commands::Up => {
            if !info.services.is_empty() {
                info.run_services().await
            } else {
                Ok(())
            }
        }

        Commands::Run { script_name } => info.run_script(&script_name).await,
    }
}
