use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowSubcommand,
}

impl WorkflowCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        match self.command {
            WorkflowSubcommand::Run(run_workflow_command) => run_workflow_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum WorkflowSubcommand {
    Run(RunWorkflowCommand),
}

#[derive(Debug, Args)]
struct RunWorkflowCommand {
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

    #[arg(long, action = clap::ArgAction::SetTrue)]
    pretty: bool,
}

impl RunWorkflowCommand {
    fn execute(self) -> Result<(), CommandError> {
        self.validate_payload_arguments()?;

        let input_value = self.input_value()?;
        let secrets_value = self.secrets_value()?;

        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("failed to build tokio runtime: {error}")))?;

        let workflow_runtime =
            superwire_core::WorkflowRuntime::<DynamicWorkflowInput, DynamicWorkflowOutput>::from_file(&self.workflow_path)
                .map_err(|error| CommandError::internal(error.to_string()))?;

        let output_value = async_runtime
            .block_on(workflow_runtime.run_with_secrets(
                DynamicWorkflowInput { fields: input_value },
                DynamicWorkflowSecrets { fields: secrets_value },
            ))
            .map_err(|error| CommandError::internal(error.to_string()))?;

        if self.pretty {
            println!(
                "{}",
                serde_json::to_string_pretty(&output_value.fields)
                    .map_err(|error| CommandError::internal(format!("failed to serialize pretty workflow output: {error}")))?
            );

            return Ok(());
        }

        println!(
            "{}",
            serde_json::to_string(&output_value.fields)
                .map_err(|error| CommandError::internal(format!("failed to serialize workflow output: {error}")))?
        );

        Ok(())
    }

    fn validate_payload_arguments(&self) -> Result<(), CommandError> {
        if self.input_json.is_some() && self.input_file.is_some() {
            return Err(CommandError::invalid_input("use either --input-json or --input-file, not both"));
        }

        if self.secrets_json.is_some() && self.secrets_file.is_some() {
            return Err(CommandError::invalid_input("use either --secrets-json or --secrets-file, not both"));
        }

        Ok(())
    }

    fn input_value(&self) -> Result<Map<String, Value>, CommandError> {
        self.payload_as_object(self.input_json.as_deref(), self.input_file.as_deref(), "input payload")
    }

    fn secrets_value(&self) -> Result<Map<String, Value>, CommandError> {
        self.payload_as_object(self.secrets_json.as_deref(), self.secrets_file.as_deref(), "secrets payload")
    }

    fn payload_as_object(
        &self,
        inline_payload: Option<&str>,
        payload_file_path: Option<&Path>,
        payload_label: &str,
    ) -> Result<Map<String, Value>, CommandError> {
        let payload_json = if let Some(inline_payload) = inline_payload {
            inline_payload.to_string()
        } else if let Some(payload_file_path) = payload_file_path {
            fs::read_to_string(payload_file_path).map_err(|error| {
                CommandError::invalid_input(format!(
                    "failed to read {payload_label} file {}: {error}",
                    payload_file_path.display()
                ))
            })?
        } else {
            "{}".to_string()
        };

        let parsed_payload_value = serde_json::from_str::<Value>(&payload_json)
            .map_err(|error| CommandError::invalid_input(format!("{payload_label} must be valid json: {error}")))?;

        let Some(parsed_payload_object) = parsed_payload_value.as_object() else {
            return Err(CommandError::invalid_input(format!("{payload_label} must be a json object")));
        };

        Ok(parsed_payload_object.clone())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
struct DynamicWorkflowInput {
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
struct DynamicWorkflowOutput {
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
struct DynamicWorkflowSecrets {
    #[serde(flatten)]
    fields: Map<String, Value>,
}
