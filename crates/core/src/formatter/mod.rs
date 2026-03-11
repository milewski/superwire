//! AST-based formatter for .ai files
//!
//! This module provides clean, consistent formatting for Engine AI workflow files
//! by parsing the AST and reassembling it with unified formatting rules.

mod formatter;
mod rules;
mod writer;

pub use formatter::{FormatResult, Formatter, FormatterError};
