use anyhow::{bail, Context};
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::Path;

pub fn init_project(path_to_toml: impl AsRef<Path>) -> anyhow::Result<()> {
    if path_to_toml.as_ref().is_file() {
        bail!(
            "File {} already exists. Aborting.",
            path_to_toml.as_ref().display()
        );
    }

    println!(
        "Setting up project file {}",
        path_to_toml.as_ref().display()
    );

    let project_dir = path_to_toml
        .as_ref()
        .parent()
        .context("unable to find parent folder")?;

    create_dir_all(project_dir).context("unable to create project folders")?;

    // Create/update gitignore
    let git_folder = project_dir.join(".git");
    let gitignore = project_dir.join(".gitignore");
    if !has_initialised_gitignore(&gitignore) && git_folder.is_dir() {
        let mut file = File::options()
            .write(true)
            .append(true)
            .create(true)
            .open(&gitignore)
            .context("unable to create .gitignore")?;

        file.write_all(b"\n/.hb-state")
            .context("unable to write to .gitignore")?;
    }

    // Create default project file at path_to_toml
    let mut file = File::create(&path_to_toml).context("unable to create project file")?;
    file.write_all(include_bytes!("project_template.toml"))
        .context("unable to write to project file")?;

    Ok(())
}

fn has_initialised_gitignore(gitignore: impl AsRef<Path>) -> bool {
    let mut file = match File::open(&gitignore) {
        Ok(file) => file,
        Err(_) => return false,
    };

    let mut file_contents = Default::default();
    match file.read_to_string(&mut file_contents) {
        Ok(_) => (),
        Err(_) => return false,
    }

    file_contents.contains("/.hb-state")
}
