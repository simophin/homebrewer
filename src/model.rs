use anyhow::{bail, Context};
use derive_more::{Deref, Display};
use indexmap::IndexMap;
use maplit::hashmap;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
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
    pub var: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct ServiceEnvironment {
    pub script: String,
    pub environ: HashMap<String, String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProjectEnvironment {
    pub environ: HashMap<String, String>,
    pub user_environ: HashMap<String, String>,
    pub scripts: HashMap<String, String>,
    pub services: HashMap<String, ServiceEnvironment>,
    pub shell_hook: Option<String>,
    pub state_dir: PathBuf,
}

#[derive(Serialize)]
struct RenderContext {
    pkg: HashMap<String, String>,
    var: HashMap<String, String>,
}

impl ProjectDesc {
    pub fn to_environment(
        &self,
        project_dir: impl AsRef<Path>,
        state_dir: impl AsRef<Path>,
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
            var: [
                (
                    String::from("project_dir"),
                    project_dir.as_ref().to_str().unwrap().to_string(),
                ),
                (
                    String::from("state_dir"),
                    state_dir.as_ref().to_str().unwrap().to_string(),
                ),
            ]
            .into_iter()
            .chain(self.var.iter().flat_map(|m| m.clone().into_iter()))
            .collect(),
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
                        working_directory: Path::new(state_dir.as_ref()).join(&name),
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
            state_dir: state_dir.as_ref().to_path_buf(),
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
