use std::fs;
use std::path::PathBuf;

use clap::Args;
use engine_ai_core::diagnostic::render_diagnostics_for_cli;
use engine_ai_core::dsl::{format_workflow_source, DslFormatError};

use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub struct FormatCommand {
    #[arg(value_name = "WORKFLOW")]
    workflow_path: PathBuf,

    #[arg(long)]
    check: bool,
}

impl FormatCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        let workflow_source = self.read_workflow_source()?;

        let formatted_source = format_workflow_source(&workflow_source)
            .map_err(|format_error| CommandError::invalid_workflow(self.render_format_error(format_error)))?;

        if self.check {
            if workflow_source == formatted_source {
                println!("workflow formatting is canonical: {}", self.workflow_path.display());
                return Ok(());
            }

            return Err(CommandError::invalid_workflow(format!(
                "workflow formatting differs from canonical style: {}",
                self.workflow_path.display()
            )));
        }

        if workflow_source != formatted_source {
            fs::write(&self.workflow_path, formatted_source).map_err(|io_error| {
                CommandError::internal(format!(
                    "failed to write formatted workflow file {}: {io_error}",
                    self.workflow_path.display()
                ))
            })?;

            println!("formatted workflow: {}", self.workflow_path.display());

            return Ok(());
        }

        println!("workflow already formatted: {}", self.workflow_path.display());

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

    fn render_format_error(&self, format_error: DslFormatError) -> String {
        match format_error {
            DslFormatError::Parse(parse_error) => {
                let parse_diagnostic = parse_error.diagnostic();
                let rendered_diagnostics = render_diagnostics_for_cli(&[parse_diagnostic], self.workflow_path.to_str());

                format!("workflow formatting failed due to syntax errors:\n{rendered_diagnostics}")
            }
            DslFormatError::LineCommentNotAllowed { line, column } => {
                format!(
                    "workflow formatting failed for {}: line comments (`//`) are not allowed (line {line}, column {column})",
                    self.workflow_path.display()
                )
            }
        }
    }
}
