use crate::model::ProjectEnvironment;
use anyhow::Context;

impl ProjectEnvironment {
    pub async fn run_script(&self, name: &str) -> anyhow::Result<()> {
        let script = self
            .scripts
            .get(name)
            .with_context(|| format!("unable to find script named '{name}'"))?;

        self.run_shell(Some(script)).await
    }
}
