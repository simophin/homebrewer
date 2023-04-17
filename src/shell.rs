use crate::model::ProjectEnvironment;
use anyhow::Context;

impl ProjectEnvironment {
    pub fn apply_current(&self) -> anyhow::Result<()> {
        for (name, value) in &self.environ {
            let value = format!("{value}:{}", std::env::var(name).unwrap_or_default());
            std::env::set_var(name, value);
        }

        std::fs::create_dir_all(&self.state_dir).context("Creating state dirs")?;

        Ok(())
    }
}
