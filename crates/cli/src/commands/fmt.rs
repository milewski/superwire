use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use superwire_core::dsl::{format_workflow_source, DslFormatError};

use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub struct FormatCommand {
    #[arg(value_name = "PATH")]
    target_path: PathBuf,
}

impl FormatCommand {
    pub fn execute(self) -> Result<(), CommandError> {
        let workflow_paths = self.collect_workflow_paths()?;

        if workflow_paths.is_empty() {
            return Err(CommandError::invalid_input(format!(
                "no workflow files (.wire) found at {}",
                self.target_path.display()
            )));
        }

        for workflow_path in workflow_paths {
            self.format_workflow_file(&workflow_path)?;
        }

        Ok(())
    }

    fn collect_workflow_paths(&self) -> Result<Vec<PathBuf>, CommandError> {
        if self.target_path.is_file() {
            if !is_workflow_file_path(&self.target_path) {
                return Err(CommandError::invalid_input(format!(
                    "expected a .wire workflow file, got {}",
                    self.target_path.display()
                )));
            }

            return Ok(vec![self.target_path.clone()]);
        }

        if self.target_path.is_dir() {
            let mut workflow_paths = Vec::new();
            collect_wire_files_recursively(&self.target_path, &mut workflow_paths)?;
            workflow_paths.sort();

            return Ok(workflow_paths);
        }

        Err(CommandError::invalid_input(format!(
            "path does not exist or is not accessible: {}",
            self.target_path.display()
        )))
    }

    fn format_workflow_file(&self, workflow_path: &Path) -> Result<(), CommandError> {
        let workflow_source = fs::read_to_string(workflow_path).map_err(|read_error| {
            CommandError::internal(format!("failed to read workflow file {}: {read_error}", workflow_path.display()))
        })?;

        let formatted_source = format_workflow_source(&workflow_source)
            .map_err(|format_error| self.render_format_error(workflow_path, &workflow_source, format_error))?;

        if workflow_source == formatted_source {
            return Ok(());
        }

        fs::write(workflow_path, formatted_source).map_err(|write_error| {
            CommandError::internal(format!(
                "failed to write formatted workflow file {}: {write_error}",
                workflow_path.display()
            ))
        })?;

        println!("formatted {}", workflow_path.display());

        Ok(())
    }

    fn render_format_error(&self, workflow_path: &Path, workflow_source: &str, format_error: DslFormatError) -> CommandError {
        match format_error {
            DslFormatError::Parse(parse_error) => CommandError::invalid_input(format!(
                "failed to format {} due to syntax errors:\n{}",
                workflow_path.display(),
                parse_error.render_for_output_target(workflow_source, &workflow_path.display().to_string())
            )),
        }
    }
}

fn is_workflow_file_path(file_path: &Path) -> bool {
    file_path.extension().and_then(|extension| extension.to_str()) == Some("wire")
}

fn collect_wire_files_recursively(directory_path: &Path, workflow_paths: &mut Vec<PathBuf>) -> Result<(), CommandError> {
    let directory_entries = fs::read_dir(directory_path)
        .map_err(|read_error| CommandError::internal(format!("failed to read directory {}: {read_error}", directory_path.display())))?;

    for directory_entry_result in directory_entries {
        let directory_entry = directory_entry_result.map_err(|read_error| {
            CommandError::internal(format!(
                "failed to read entry in directory {}: {read_error}",
                directory_path.display()
            ))
        })?;

        let entry_path = directory_entry.path();

        if entry_path.is_dir() {
            collect_wire_files_recursively(&entry_path, workflow_paths)?;

            continue;
        }

        if entry_path.extension().and_then(|extension| extension.to_str()) != Some("wire") {
            continue;
        }

        workflow_paths.push(entry_path);
    }

    Ok(())
}
