use crate::model::{ProjectDesc, VersionSpec};
use anyhow::Context;
use std::process::Command;

pub fn install_dependencies(desc: &ProjectDesc) -> anyhow::Result<()> {
    let brew_prefix = Command::new("brew")
        .arg("--prefix")
        .output()
        .context("getting brew prefix")?
        .stdout;

    let brew_prefix = std::str::from_utf8(&brew_prefix)
        .context("converting brew prefix to UTF-8")?
        .trim();

    let mut brew_cmd = Command::new("brew");
    brew_cmd.arg("info").arg("--json");

    for (n, v) in &desc.dependencies {
        brew_cmd.arg(dependency_name(n, v));
    }

    let output = brew_cmd.output().context("Error executing brew command")?;

    // let result: Vec<DependencyInfo> =
    //     serde_json::from_slice(&output.stdout).context("Parsing brew output")?;
    //
    // // Install missing dependencies
    // let missing: Vec<_> = result.iter().filter(|d| d.installed.is_none()).collect();
    // if !missing.is_empty() {
    //     println!(
    //         "Installing dependencies {}",
    //         missing.iter().map(|m| m.name).collect::<Vec<_>>().join(",")
    //     );
    //     let mut install_cmd = Command::new("brew");
    //     install_cmd.arg("install");
    //
    //     for DependencyInfo { name, .. } in missing {
    //         install_cmd.arg(name);
    //     }
    //
    //     install_cmd.output().context("Error running brew install")?;
    // } else {
    //     println!("All dependencies are installed")
    // }

    todo!()
}

fn dependency_name(name: impl AsRef<str>, version: &VersionSpec) -> String {
    match version {
        VersionSpec::Latest => name.as_ref().to_string(),
        VersionSpec::Versioned(v) => format!("{}@{v}", name.as_ref()),
    }
}
