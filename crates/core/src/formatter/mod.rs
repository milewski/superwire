//! Code formatter for .ai workflow files
//!
//! This module provides functionality to format .ai files according to
//! consistent style guidelines and syntax rules.

pub mod error;
pub mod formatter;

pub use error::FormatterError;
pub use formatter::{Formatter, FormatterConfig, FormatResult};

/// Result type for formatter operations
pub type FormatterResult<T> = Result<T, FormatterError>;
