use crate::parser::AstBuilder;
use std::fs;
use std::path::Path;
use thiserror::Error;

use super::writer::Writer;
use super::rules::{RuleEngine, SpacingRule, IndentationRule, LineBreaksRule, ArrayFormattingRule, StringFormattingRule};

#[derive(Debug, Clone)]
pub struct FormatResult {
    pub content: String,
    pub changed: bool,
}

#[derive(Debug, Error)]
pub enum FormatterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid file extension. Expected .ai file")]
    InvalidExtension,
    #[error("Formatting rule error: {0}")]
    Rule(#[from] super::rules::FormattingError),
}

pub struct Formatter {
    writer: Writer,
    rule_engine: RuleEngine,
}

impl Formatter {
    #[must_use]
    pub fn new() -> Self {
        let rule_engine = RuleEngine::new()
            .add_rule(SpacingRule::new())
            .add_rule(IndentationRule::new())
            .add_rule(LineBreaksRule::new())
            .add_rule(ArrayFormattingRule::new())
            .add_rule(StringFormattingRule::new());

        Self {
            writer: Writer::new(),
            rule_engine,
        }
    }

    /// Format a single .ai file
    pub fn format_file(&self, file_path: &Path) -> Result<FormatResult, FormatterError> {
        // Validate file extension
        if file_path.extension().is_none_or(|ext| ext != "ai") {
            return Err(FormatterError::InvalidExtension);
        }

        // Read the file content
        let original_content = fs::read_to_string(file_path)?;

        // Parse the workflow
        let builder = AstBuilder::new(file_path.to_string_lossy().to_string());
        let mut workflow = builder
            .parse(&original_content)
            .map_err(|error| FormatterError::Parse(format!("{error}")))?;

        // Apply formatting rules
        self.rule_engine.apply(&mut workflow)?;

        // Format the workflow
        let formatted_content = self.writer.write_workflow(&workflow);

        // Check if content changed
        let changed = original_content.trim() != formatted_content.trim();

        Ok(FormatResult {
            content: formatted_content,
            changed,
        })
    }

    /// Write formatted content to a file
    pub fn write_file(&self, file_path: &Path, content: &str) -> Result<(), FormatterError> {
        fs::write(file_path, content)?;
        Ok(())
    }

    /// Format all .ai files in a directory
    pub fn format_directory(&self, directory_path: &Path) -> Result<Vec<std::path::PathBuf>, FormatterError> {
        let mut formatted_files = Vec::new();

        for entry in fs::read_dir(directory_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some_and(|ext| ext == "ai") {
                let result = self.format_file(&path)?;
                if result.changed {
                    self.write_file(&path, &result.content)?;
                    formatted_files.push(path);
                }
            } else if path.is_dir() {
                // Recursively format subdirectories
                let mut sub_formatted = self.format_directory(&path)?;
                formatted_files.append(&mut sub_formatted);
            }
        }

        Ok(formatted_files)
    }

    /// Check formatting of all .ai files in a directory without modifying them
    pub fn check_directory(&self, directory_path: &Path) -> Result<Vec<std::path::PathBuf>, FormatterError> {
        let mut unformatted_files = Vec::new();

        for entry in fs::read_dir(directory_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some_and(|ext| ext == "ai") {
                let result = self.format_file(&path)?;
                if result.changed {
                    unformatted_files.push(path);
                }
            } else if path.is_dir() {
                // Recursively check subdirectories
                let mut sub_unformatted = self.check_directory(&path)?;
                unformatted_files.append(&mut sub_unformatted);
            }
        }

        Ok(unformatted_files)
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}
