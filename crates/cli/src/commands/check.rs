use std::path::PathBuf;

use clap::Args;

use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub struct CheckCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,
}

impl CheckCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        if !self.workflow_path.exists() {
            return Err(CommandError::invalid_workflow(format!(
                "workflow file does not exist: {}",
                self.workflow_path.display()
            )));
        }

        Ok(())
    }
}
