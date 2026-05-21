use std::fs;
use std::path::PathBuf;

use clap::Args;
use serde_json::Value;
use superwire_core::dsl::parse_workflow;
use superwire_executor::{CerseiModelProvider, ExecutorError, WorkflowExecutor};

use super::json::WorkflowPayloadSources;
use super::schema::CliRuntimeSchemaContext;
use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub(super) struct RunWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH")]
    workflow_path: PathBuf,

    #[arg(long, value_name = "JSON")]
    input_json: Option<String>,

    #[arg(long, value_name = "INPUT_JSON_FILE")]
    input_file: Option<PathBuf>,

    #[arg(long, value_name = "JSON")]
    secrets_json: Option<String>,

    #[arg(long, value_name = "SECRETS_JSON_FILE")]
    secrets_file: Option<PathBuf>,

    #[arg(long = "set", value_name = "KEY=VALUE", number_of_values = 1)]
    set: Option<Vec<String>>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    pretty: bool,
}

impl RunWorkflowCommand {
    pub(super) fn execute(self) -> Result<(), CommandError> {
        self.payload_sources().validate()?;

        let input_value = self.payload_sources().input_value()?;
        let secrets_value = self.payload_sources().secrets_value()?;

        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("failed to build tokio runtime: {error}")))?;

        let workflow_source = fs::read_to_string(&self.workflow_path)
            .map_err(|error| CommandError::internal(format!("failed to read workflow file {}: {error}", self.workflow_path.display())))?;

        let parsed_workflow = parse_workflow(&workflow_source).map_err(|error| {
            CommandError::internal(error.render_for_output_target(&workflow_source, &self.workflow_path.display().to_string()))
        })?;

        let _runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)?;
        let workflow_executor =
            WorkflowExecutor::from_source(&workflow_source).map_err(|error| CommandError::internal(error.to_string()))?;

        let output_value = async_runtime
            .block_on(workflow_executor.execute(
                Value::Object(input_value),
                Value::Object(secrets_value),
                &CerseiModelProvider,
                None,
                10,
            ))
            .map_err(Self::map_workflow_runtime_error)?;

        if self.pretty {
            println!(
                "{}",
                serde_json::to_string_pretty(&output_value)
                    .map_err(|error| CommandError::internal(format!("failed to serialize pretty workflow output: {error}")))?
            );

            return Ok(());
        }

        println!(
            "{}",
            serde_json::to_string(&output_value)
                .map_err(|error| CommandError::internal(format!("failed to serialize workflow output: {error}")))?
        );

        Ok(())
    }

    fn payload_sources(&self) -> WorkflowPayloadSources<'_> {
        WorkflowPayloadSources::new(
            self.input_json.as_deref(),
            self.input_file.as_deref(),
            self.secrets_json.as_deref(),
            self.secrets_file.as_deref(),
            self.set.as_deref(),
        )
    }

    fn map_workflow_runtime_error(error: ExecutorError) -> CommandError {
        CommandError::internal_with_details(
            error.to_string(),
            serde_json::json!({
                "type": "workflow_runtime_error",
                "error": error.to_string(),
            }),
        )
    }
}
