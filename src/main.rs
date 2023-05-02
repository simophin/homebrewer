use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::model::{ProjectDesc, ProjectEnvironment};
use anyhow::{bail, Context};
use clap::{Parser, Subcommand};

mod direnv;
mod init;
mod model;
mod run;
mod ser;
mod service;
mod shell;
mod utils;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[clap(short, default_value = "devit.toml")]
    /// Use this TOML
    path_to_toml: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up a new project
    Init,

    /// Spin up a shell with the environment set up
    #[clap(trailing_var_arg = true)]
    Shell {
        /// The script to run, interactive shell if not given
        args: Option<Vec<String>>,
    },

    /// Bring up services
    Up {
        /// The services to bring up. Default to all services if empty.
        service_names: Option<Vec<String>>,
    },

    /// Run a particular script
    Run { script_name: String },

    /// Install the necessary dependencies
    Install,

    /// Print commands for direnv to set up the environment
    Direnv,

    /// Print out the project information in json format
    Info,
}

async fn read_project(toml_file: impl AsRef<Path>) -> anyhow::Result<ProjectEnvironment> {
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
            project_dir
                .join(".devit-state")
                .to_str()
                .unwrap_or_default(),
        )
        .await
        .context("environment")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        command,
        path_to_toml,
    } = Cli::parse();

    match command {
        Commands::Init => init::init_project(path_to_toml),
        Commands::Shell { args } => {
            read_project(&path_to_toml)
                .await
                .context("reading project file")?
                .run_shell(args.map(|args| args.join(" ")))
                .await
        }

        Commands::Up { service_names } => {
            let info = read_project(&path_to_toml)
                .await
                .context("reading project file")?;
            if !info.services.is_empty() {
                info.run_services(service_names).await
            } else {
                Ok(())
            }
        }

        Commands::Run { script_name } => {
            read_project(&path_to_toml)
                .await
                .context("reading project file")?
                .run_script(&script_name)
                .await
        }

        Commands::Install => {
            let status = read_project(&path_to_toml)
                .await
                .context("reading project file")?
                .run_command("sh", false)
                .arg("-c")
                .arg("echo All dependencies installed")
                .spawn()
                .context("Running install")?
                .wait()
                .await
                .context("wait for sh to finish")?;

            if !status.success() {
                bail!("Unable to install dependencies")
            }

            Ok(())
        }

        Commands::Info => serde_json::to_writer_pretty(
            std::io::stdout(),
            &read_project(&path_to_toml)
                .await
                .context("reading project file")?,
        )
        .context("writing json"),

        Commands::Direnv => {
            direnv::print_direnv_commands(
                &read_project(&path_to_toml)
                    .await
                    .context("reading project file")?,
            )
            .await
        }
    }
}
