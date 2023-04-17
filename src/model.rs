use anyhow::{bail, Context};
use derive_more::{Deref, Display};
use indexmap::IndexMap;
use maplit::hashmap;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::{format, Display, Formatter};
use std::fs::{read_dir, ReadDir};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Display, Deserialize, Serialize, Eq, PartialEq, Deref)]
pub struct TemplatedString(String);

#[derive(Debug)]
pub enum VersionSpec {
    Latest,
    Versioned(String),
}

impl Display for VersionSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionSpec::Latest => Ok(()),
            VersionSpec::Versioned(v) => f.write_fmt(format_args!("@{v}")),
        }
    }
}

impl<'de> Deserialize<'de> for VersionSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <String as Deserialize<'_>>::deserialize(deserializer)?;
        if s.eq_ignore_ascii_case("latest") {
            Ok(Self::Latest)
        } else {
            Ok(Self::Versioned(s))
        }
    }
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
    pub path: TemplatedString,
    pub args: Option<Vec<TemplatedString>>,
    pub init_script: Option<TemplatedString>,
    pub env: Option<HashMap<String, TemplatedString>>,
}

#[derive(Deserialize, Debug)]
pub struct ProjectDesc {
    pub shell: Option<ShellConfig>,
    pub dependencies: IndexMap<String, VersionSpec>,
    pub env: Option<HashMap<String, TemplatedString>>,
    pub services: Option<HashMap<String, ServiceConfig>>,
    pub scripts: Option<HashMap<String, TemplatedString>>,
}

#[derive(Debug)]
pub struct ServiceEnvironment {
    pub program: String,
    pub args: Vec<String>,
    pub environ: HashMap<String, String>,
    pub init_script: Option<String>,
}

#[derive(Debug)]
pub struct ProjectEnvironment {
    pub environ: HashMap<String, String>,
    pub scripts: HashMap<String, String>,
    pub services: HashMap<String, ServiceEnvironment>,
    pub shell_hook: Option<String>,
    pub state_dir: String,
}

#[derive(Serialize)]
struct RenderContext {
    project_dir: String,
    state_dir: String,
    pkg: HashMap<String, String>,
}

impl ProjectDesc {
    pub fn to_environment(
        &self,
        project_dir: impl AsRef<str>,
        state_dir: impl AsRef<str>,
    ) -> anyhow::Result<ProjectEnvironment> {
        let prefix = Command::new("brew")
            .arg("--prefix")
            .output()
            .context("getting prefix")?;

        if !prefix.status.success() {
            bail!(
                "Error getting prefix: {}",
                std::str::from_utf8(&prefix.stderr).unwrap_or_default()
            );
        }

        let prefix = std::str::from_utf8(&prefix.stdout)
            .context("getting prefix")?
            .trim();
        println!("Using homebrew prefix {prefix}");

        let prefix = Path::new(prefix);

        let dependency_prefixes = self
            .dependencies
            .iter()
            .map(|(k, v)| {
                let name = format!("{k}{v}");
                let p = prefix.join("opt").join(&name);
                (k.clone(), name, p)
            })
            .collect::<Vec<_>>();

        let to_install = dependency_prefixes
            .iter()
            .filter(|(_, _, p)| !p.exists())
            .map(|(_, n, p)| {
                println!("Prefix {p:?} doesn't exist");
                n.clone()
            })
            .collect::<Vec<_>>();

        if !to_install.is_empty() {
            println!("Installing {}", to_install.join(", "));
            let mut cmd = Command::new("brew");
            cmd.arg("install");
            for i in to_install {
                cmd.arg(i);
            }

            let output = cmd
                .spawn()
                .context("Running brew install")?
                .wait_with_output()
                .context("Waiting for brew install output")?;

            if !output.status.success() {
                bail!("Failed installing missing dependencies");
            }
        }

        let render_context = RenderContext {
            project_dir: project_dir.as_ref().to_string(),
            state_dir: state_dir.as_ref().to_string(),
            pkg: dependency_prefixes
                .iter()
                .filter_map(|(n, _, p)| p.to_str().map(|v| (n.to_string(), v.to_string())))
                .collect(),
        };

        let path = self
            .shell
            .as_ref()
            .and_then(|s| s.user_paths.as_ref())
            .unwrap_or(&vec![])
            .iter()
            .map(|t| render_template(t, &render_context).expect("To render"))
            .chain(
                dependency_prefixes
                    .iter()
                    .filter_map(|(_, _, p)| p.join("bin").to_str().map(|s| s.to_string())),
            )
            .collect::<Vec<_>>()
            .join(":");

        let lib_path = dependency_prefixes
            .iter()
            .filter_map(|(_, _, p)| p.join("lib").to_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(":");

        let include_path = dependency_prefixes
            .iter()
            .filter_map(|(_, _, p)| p.join("include").to_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(":");

        let mut environ = hashmap! {
            String::from("PATH") => path,
            String::from("LIBRARY_PATH") => lib_path,
            String::from("C_INCLUDE_PATH") => include_path.clone(),
            String::from("CPLUS_INCLUDE_PATH") => include_path,
        };

        if let Some(env) = &self.env {
            for (k, v) in env {
                environ.insert(
                    k.clone(),
                    render_template(v, &render_context).context("rendering")?,
                );
            }
        }

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
            .map(
                |(
                    name,
                    ServiceConfig {
                        path,
                        args,
                        init_script,
                        env,
                    },
                )| {
                    (
                        name.clone(),
                        ServiceEnvironment {
                            program: render_template(path, &render_context)
                                .expect("to render path"),
                            args: args
                                .iter()
                                .flat_map(|v| v.iter())
                                .map(|t| {
                                    render_template(t, &render_context).expect("to render arg")
                                })
                                .collect(),
                            environ: env
                                .iter()
                                .flat_map(|v| v.iter())
                                .map(|(name, tpl)| {
                                    (
                                        name.clone(),
                                        render_template(tpl, &render_context)
                                            .expect("to render env"),
                                    )
                                })
                                .collect(),
                            init_script: init_script.as_ref().map(|t| {
                                render_template(t, &render_context).expect("to render init script")
                            }),
                        },
                    )
                },
            )
            .collect();

        Ok(ProjectEnvironment {
            environ,
            scripts,
            services,
            shell_hook: self
                .shell
                .as_ref()
                .and_then(|s| s.hook.as_ref())
                .map(|t| render_template(t, &render_context).expect("to render hook")),
            state_dir: state_dir.as_ref().to_string(),
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
