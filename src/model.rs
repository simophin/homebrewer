use anyhow::Context;
use derive_more::{Deref, Display};
use indexmap::IndexMap;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

use crate::utils::brew_prefixes;

#[derive(Debug, Display, Deserialize, Serialize, Eq, PartialEq, Deref)]
pub struct TemplatedString(String);

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionSpec {
    // postgresql = "12"
    // postgresql = "*"
    // postgresql = ""
    VersionOnly(String),

    // elasticsearch = { name = "elastic/tap/elasticsearch-full" }
    // elasticsearch = { name = "elastic/tap/elasticsearch-full", version = "*" }
    Full {
        name: String,
        version: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellConfig {
    pub user_paths: Option<Vec<TemplatedString>>,
    pub hook: Option<TemplatedString>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceConfig {
    pub script: TemplatedString,
    pub env: Option<HashMap<String, TemplatedString>>,
}

#[derive(Deserialize, Debug)]
pub struct ProjectDesc {
    pub shell: Option<ShellConfig>,
    pub dependencies: IndexMap<String, VersionSpec>,
    pub env: Option<HashMap<String, TemplatedString>>,
    pub services: Option<HashMap<String, ServiceConfig>>,
    pub scripts: Option<HashMap<String, TemplatedString>>,
    pub vars: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceEnvironment {
    pub script: String,
    pub environ: HashMap<String, String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectEnvironment {
    pub environ: HashMap<String, String>,
    pub user_environ: HashMap<String, String>,
    pub scripts: HashMap<String, String>,
    pub services: HashMap<String, ServiceEnvironment>,
    pub shell_hook: Option<String>,
    pub state_dir: PathBuf,
}

#[derive(Serialize, Clone)]
struct DependencyInfo {
    name: String,
    path: PathBuf,
}

#[derive(Serialize)]
struct RenderContext {
    state_dir: PathBuf,
    project_dir: PathBuf,
    pkgs: IndexMap<String, DependencyInfo>,
    vars: IndexMap<String, String>,
}

impl VersionSpec {
    pub fn to_brew_name<'a>(&'a self, key: &'a str) -> Cow<'a, str> {
        match self {
            VersionSpec::VersionOnly(s) if s.trim() == "*" || s.trim().is_empty() => {
                Cow::Borrowed(key.trim())
            }
            VersionSpec::VersionOnly(s) => Cow::Owned(format!("{}@{}", key.trim(), s.trim())),
            VersionSpec::Full {
                name,
                version: Some(version),
            } if !version.trim().is_empty() => {
                Cow::Owned(format!("{}@{}", name.trim(), version.trim()))
            }
            VersionSpec::Full { name, .. } => Cow::Borrowed(name.trim()),
        }
    }
}

impl ProjectDesc {
    pub async fn to_environment(
        &self,
        project_dir: impl AsRef<Path>,
        state_dir: impl AsRef<Path>,
    ) -> anyhow::Result<ProjectEnvironment> {
        let project_dir = project_dir.as_ref().to_path_buf();
        let state_dir = state_dir.as_ref().to_path_buf();

        if project_dir.is_relative() {
            panic!("Project dir can not be relative")
        }

        if state_dir.is_relative() {
            panic!("State dir can not be relative")
        }

        let pkgs: IndexMap<String, DependencyInfo> = brew_prefixes(
            self.dependencies
                .iter()
                .map(|(key, spec)| spec.to_brew_name(key.as_str()).into_owned()),
        )
        .await
        .context("getting brew prefixes")?
        .into_iter()
        .zip(self.dependencies.iter())
        .map(|(prefix, (key, spec))| {
            (
                key.clone(),
                DependencyInfo {
                    name: spec.to_brew_name(key.as_str()).into_owned(),
                    path: prefix.into(),
                },
            )
        })
        .collect();

        let to_install = pkgs
            .values()
            .filter(|info| !Path::new(&info.path).exists())
            .map(|info| {
                println!("{} doesn't exist", &info.name);
                info.name.as_str()
            })
            .collect::<Vec<_>>();

        if !to_install.is_empty() {
            println!("Installing {}", to_install.join(", "));
            let output = Command::new("brew")
                .arg("install")
                .args(to_install.iter())
                .spawn()
                .context("Running brew install")?
                .wait_with_output()
                .context("Waiting for brew install output")?;

            if !output.status.success() {
                exit(output.status.code().unwrap_or(-1));
            }
        }

        let render_context = RenderContext {
            project_dir,
            state_dir: state_dir.clone(),
            vars: self
                .vars
                .as_ref()
                .iter()
                .flat_map(|s| s.iter())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            pkgs: pkgs.clone(),
        };

        let path = self
            .shell
            .as_ref()
            .and_then(|s| s.user_paths.as_ref())
            .iter()
            .flat_map(|v| v.iter())
            .map(|t| render_template(t, &render_context).expect("To render"))
            .chain(pkgs.iter().flat_map(|(_, info)| {
                ["bin", "sbin"].iter().map(|sub| {
                    info.path
                        .join(sub)
                        .to_str()
                        .expect("path to string")
                        .to_string()
                })
            }))
            .collect::<Vec<_>>()
            .join(":");

        let lib_path = pkgs
            .iter()
            .filter_map(|(_, p)| p.path.join("lib").to_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(":");

        let include_path = pkgs
            .iter()
            .filter_map(|(_, p)| p.path.join("include").to_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(":");

        let environ = hashmap! {
            String::from("PATH") => path,
            String::from("LIBRARY_PATH") => lib_path,
            String::from("C_INCLUDE_PATH") => include_path.clone(),
            String::from("CPLUS_INCLUDE_PATH") => include_path,
        };

        let user_environ = self
            .env
            .iter()
            .flat_map(|m| m.iter())
            .map(|(n, v)| {
                (
                    n.clone(),
                    render_template(v, &render_context).expect("to render environment"),
                )
            })
            .collect();

        let scripts = self
            .scripts
            .as_ref()
            .iter()
            .flat_map(|v| v.iter())
            .map(|(name, script)| {
                (
                    name.to_string(),
                    render_template(script, &render_context).expect("to render"),
                )
            })
            .collect();

        let services = self
            .services
            .as_ref()
            .iter()
            .flat_map(|v| v.iter())
            .map(|(name, ServiceConfig { script, env })| {
                (
                    name.clone(),
                    ServiceEnvironment {
                        environ: env
                            .iter()
                            .flat_map(|v| v.iter())
                            .map(|(name, tpl)| {
                                (
                                    name.clone(),
                                    render_template(tpl, &render_context).expect("to render env"),
                                )
                            })
                            .collect(),
                        script: render_template(script, &render_context).expect("to render script"),
                        working_directory: state_dir.join(&name),
                    },
                )
            })
            .collect();

        Ok(ProjectEnvironment {
            environ,
            user_environ,
            scripts,
            services,
            shell_hook: self
                .shell
                .as_ref()
                .and_then(|s| s.hook.as_ref())
                .map(|t| render_template(t, &render_context).expect("to render hook")),
            state_dir,
        })
    }
}

fn render_template(tpl: &TemplatedString, context: impl Serialize) -> anyhow::Result<String> {
    let mut template = tinytemplate::TinyTemplate::new();
    template
        .add_template("default", tpl.as_str())
        .context("Adding template")?;

    template
        .render("default", &context)
        .context("Rendering template")
}
