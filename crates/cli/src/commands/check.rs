use std::fs;
use std::path::PathBuf;

use clap::Args;
use engine_ai_core::diagnostic::render_diagnostics_for_cli;
use engine_ai_core::dsl::{parse_workflow, validate_workflow, DslParseError};
use engine_ai_core::semantic::{compile_dynamic_workflow, WorkflowPipelineInput};
use engine_ai_core::WorkflowRuntimeError;

use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub struct CheckCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,
}

impl CheckCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        let workflow_source = self.read_workflow_source()?;

        match compile_dynamic_workflow(WorkflowPipelineInput::Source(&workflow_source)) {
            Ok(_) => {
                println!("workflow is valid: {}", self.workflow_path.display());

                Ok(())
            }
            Err(workflow_runtime_error) => Err(CommandError::invalid_workflow(
                self.render_compile_failure_message(&workflow_source, workflow_runtime_error),
            )),
        }
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

    fn render_compile_failure_message(&self, workflow_source: &str, workflow_runtime_error: WorkflowRuntimeError) -> String {
        match workflow_runtime_error {
            WorkflowRuntimeError::ParseFailed { source } => self.render_parse_failure(&source),
            WorkflowRuntimeError::InvalidWorkflow { issues } => self.render_validation_failure(workflow_source, &issues),
            other_compile_error => {
                format!(
                    "workflow static compilation failed for {}: {other_compile_error}",
                    self.workflow_path.display()
                )
            }
        }
    }

    fn render_parse_failure(&self, parse_error: &DslParseError) -> String {
        let parse_diagnostic = parse_error.diagnostic();
        let rendered_diagnostics = render_diagnostics_for_cli(&[parse_diagnostic], self.workflow_path.to_str());

        format!("workflow syntax check failed:\n{rendered_diagnostics}")
    }

    fn render_validation_failure(&self, workflow_source: &str, fallback_issues: &str) -> String {
        let Ok(workflow) = parse_workflow(workflow_source) else {
            return format!("workflow validation failed:\n{fallback_issues}");
        };

        let validation_report = validate_workflow(&workflow);

        if !validation_report.has_issues() {
            return format!("workflow validation failed:\n{fallback_issues}");
        }

        let rendered_diagnostics = render_diagnostics_for_cli(&validation_report.diagnostics(), self.workflow_path.to_str());

        format!("workflow validation failed:\n{rendered_diagnostics}")
    }
}
