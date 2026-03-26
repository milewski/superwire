use std::fs;
use std::path::PathBuf;

use clap::Args;

use crate::diagnostics::CommandError;
use crate::execution::execute_workflow_from_source;

#[derive(Debug, Args)]
pub struct RunCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0.., value_name = "INPUT_ARGS")]
    invocation_arguments: Vec<String>,
}

impl RunCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        let workflow_source = self.read_workflow_source()?;
        let workflow_output = execute_workflow_from_source(&workflow_source, &self.invocation_arguments)?;
        let rendered_output = serde_json::to_string_pretty(&workflow_output)
            .map_err(|error| CommandError::internal(format!("failed to serialize workflow output: {error}")))?;

        println!("{rendered_output}");

        Ok(())
    }

    fn read_workflow_source(&self) -> Result<String, CommandError> {
        if !self.workflow_path.exists() {
            return Err(CommandError::invalid_workflow(format!(
                "workflow file does not exist: {}",
                self.workflow_path.display()
            )));
        }

        fs::read_to_string(&self.workflow_path).map_err(|io_error| {
            CommandError::internal(format!("failed to read workflow file {}: {io_error}", self.workflow_path.display()))
        })
    }
}
