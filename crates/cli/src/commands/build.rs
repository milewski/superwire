use std::path::PathBuf;

use clap::Args;

use crate::diagnostics::CommandError;
use crate::input::SecretAssignment;

#[derive(Debug, Args)]
pub struct BuildCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,

    #[arg(long, value_name = "OUTPUT")]
    output_path: PathBuf,

    #[arg(long = "secret", value_name = "NAME=VALUE")]
    secret_assignments: Vec<String>,
}

impl BuildCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        if !self.workflow_path.exists() {
            return Err(CommandError::invalid_workflow(format!(
                "workflow file does not exist: {}",
                self.workflow_path.display()
            )));
        }

        if let Some(output_parent_directory) = self.output_path.parent() {
            if !output_parent_directory.as_os_str().is_empty() && !output_parent_directory.exists() {
                return Err(CommandError::invalid_workflow(format!(
                    "output directory does not exist: {}",
                    output_parent_directory.display()
                )));
            }
        }

        self.secret_assignments
            .into_iter()
            .map(|secret_assignment| SecretAssignment::parse(&secret_assignment))
            .collect::<Result<Vec<_>, _>>()?;

        Err(CommandError::runtime_not_implemented("build"))
    }
}
