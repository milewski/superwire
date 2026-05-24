use std::fs;
use std::path::PathBuf;

use clap::Args;
use superwire_dsl::parse_workflow;
use superwire_executor::{ExecutorError, WorkflowExecutor};

use super::schema::CliRuntimeSchemaContext;
use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub(super) struct CheckWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH")]
    workflow_path: PathBuf,
}

impl CheckWorkflowCommand {
    pub(super) fn execute(self) -> Result<(), CommandError> {
        let workflow_source = fs::read_to_string(&self.workflow_path).map_err(|read_error| {
            CommandError::invalid_input(format!(
                "failed to read workflow file {}: {read_error}",
                self.workflow_path.display()
            ))
        })?;

        let parsed_workflow = parse_workflow(&workflow_source).map_err(|parse_error| {
            CommandError::invalid_input(parse_error.render_for_output_target(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let _runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)
            .map_err(|schema_context_error| CommandError::invalid_input(schema_context_error.message().to_string()))?;
        WorkflowExecutor::from_source(&workflow_source).map_err(Self::map_workflow_runtime_error)?;

        println!("workflow is valid");

        Ok(())
    }

    fn map_workflow_runtime_error(runtime_error: ExecutorError) -> CommandError {
        CommandError::invalid_input(runtime_error.to_string())
    }
}
