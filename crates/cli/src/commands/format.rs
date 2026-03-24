use crate::error::CliError;
use crate::formatter::format_source;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FormatCommand {
    source_file_path: PathBuf,
}

impl FormatCommand {
    pub fn parse(mut command_arguments: impl Iterator<Item = String>, usage: String) -> Result<Self, CliError> {
        let Some(source_file_argument) = command_arguments.next() else {
            return Err(CliError::MissingFormatPath { usage });
        };

        if let Some(unexpected_argument) = command_arguments.next() {
            return Err(CliError::UnexpectedFormatArgument {
                argument: unexpected_argument,
                usage,
            });
        }

        Ok(Self {
            source_file_path: PathBuf::from(source_file_argument),
        })
    }

    pub fn execute(self) -> Result<(), CliError> {
        let source_file_display_path = display_path(&self.source_file_path);

        let source_file_contents = std::fs::read_to_string(&self.source_file_path).map_err(|read_error| CliError::ReadSourceFile {
            path: source_file_display_path.clone(),
            source: read_error,
        })?;

        let formatted_source_file_contents = format_source(&source_file_contents).map_err(|parse_error| CliError::ParseSourceFile {
            path: source_file_display_path.clone(),
            source: parse_error,
        })?;

        if source_file_contents == formatted_source_file_contents {
            return Ok(());
        }

        std::fs::write(&self.source_file_path, formatted_source_file_contents).map_err(|write_error| CliError::WriteSourceFile {
            path: source_file_display_path,
            source: write_error,
        })?;

        Ok(())
    }
}

fn display_path(source_file_path: &Path) -> String {
    source_file_path.display().to_string()
}
