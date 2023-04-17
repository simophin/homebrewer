use crate::model::{ProjectDesc, VersionSpec};
use anyhow::{bail, Context};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct DependencyInfo<'a> {
    name: &'a str,
    installed: Option<Vec<serde_json::Value>>,
}

pub fn run_as(desc: &ProjectDesc, cmd: impl AsRef<str>) -> anyhow::Result<()> {
    //
    //
    // let mut path = std::env::var("PATH").unwrap_or_default();
    // let mut lib_path = std::env::var("LIBRARY_PATH").unwrap_or_default();
    // let mut c_include_path = std::env::var("C_INCLUDE_PATH").unwrap_or_default();
    // let mut cxx_include_path = std::env::var("CPLUS_INCLUDE_PATH").unwrap_or_default();
    //
    // for dep in result {
    //     let name = dep.name;
    //
    //     path = format!("{brew_prefix}/opt/{name}/bin:{path}");
    //     lib_path = format!("{brew_prefix}/opt/{name}/lib:{lib_path}");
    //     c_include_path = format!("{brew_prefix}/opt/{name}/include:{c_include_path}");
    //     cxx_include_path = format!("{brew_prefix}/opt/{name}/include:{cxx_include_path}");
    // }
    //
    // std::env::set_var("PATH", &path);
    // std::env::set_var("LIBRARY_PATH", &lib_path);
    // std::env::set_var("C_INCLUDE_PATH", &c_include_path);
    // std::env::set_var("CPLUS_INCLUDE_PATH", &cxx_include_path);
    //
    // if let Some(env) = desc.env.as_ref() {
    //     for (key, value) in env {
    //         std::env::set_var(key, value)
    //     }
    // }
    //
    // let err = exec::execvp::<_, Vec<&str>>(cmd.as_ref(), vec![]);
    // bail!(err.to_string())
    todo!()
}
