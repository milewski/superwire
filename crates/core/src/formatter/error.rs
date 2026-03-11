//! Error types for the formatter module

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during formatting operations
#[derive(Error, Debug)]
pub enum FormatterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to read file '{path}': {source}")]
    FileRead { path: PathBuf, source: std::io::Error },

    #[error("Failed to write file '{path}': {source}")]
    FileWrite { path: PathBuf, source: std::io::Error },

    #[error("Invalid file extension for '{path}': expected .ai")]
    InvalidExtension { path: PathBuf },

    #[error("Parse error in file '{path}': {message}")]
    ParseError { path: PathBuf, message: String },

    #[error("Formatting failed for file '{path}': {message}")]
    FormatError { path: PathBuf, message: String },

    #[error("Directory '{path}' does not exist")]
    DirectoryNotFound { path: PathBuf },

    #[error("Path '{path}' is not a directory")]
    NotADirectory { path: PathBuf },

    #[error("No .ai files found in directory '{path}'")]
    NoFilesFound { path: PathBuf },
}
