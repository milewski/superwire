use std::path::PathBuf;

use clap::Args;

use crate::diagnostics::CommandError;
use crate::input::SecretAssignment;

#[derive(Debug, Args)]
pub struct RunCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,

    #[arg(long = "secret", value_name = "NAME=VALUE")]
    secret_assignments: Vec<String>,
}

impl RunCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        if !self.workflow_path.exists() {
            return Err(CommandError::invalid_workflow(format!(
                "workflow file does not exist: {}",
                self.workflow_path.display()
            )));
        }

        self.secret_assignments
            .into_iter()
            .map(|secret_assignment| SecretAssignment::parse(&secret_assignment))
            .collect::<Result<Vec<_>, _>>()?;

        Err(CommandError::runtime_not_implemented("run"))
    }
}
