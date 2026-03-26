use std::fs;
use std::path::PathBuf;

use clap::Args;
use engine_ai_core::semantic::{compile_dynamic_workflow, WorkflowPipelineInput};
use engine_ai_core::DynamicWorkflowRuntime;
use tokio::runtime::Runtime;

use crate::diagnostics::CommandError;
use crate::input::parse_workflow_invocation_bindings;

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
        let dynamic_compiled_workflow = compile_dynamic_workflow(WorkflowPipelineInput::Source(&workflow_source)).map_err(|error| {
            CommandError::invalid_workflow(format!(
                "workflow static compilation failed for {}: {error}",
                self.workflow_path.display()
            ))
        })?;

        let workflow_invocation_bindings = parse_workflow_invocation_bindings(
            &self.invocation_arguments,
            dynamic_compiled_workflow.input_type(),
            dynamic_compiled_workflow.typed_workflow_ir().secrets_type.as_ref(),
        )?;

        let runtime = DynamicWorkflowRuntime::from_compiled_workflow(dynamic_compiled_workflow);
        let tokio_runtime =
            Runtime::new().map_err(|error| CommandError::internal(format!("failed to initialize tokio runtime: {error}")))?;
        let execution_result = tokio_runtime.block_on(runtime.run(
            workflow_invocation_bindings.input_values,
            workflow_invocation_bindings.secret_values,
        ));

        let workflow_output = execution_result.map_err(|error| CommandError::runtime(error.to_string()))?;
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
