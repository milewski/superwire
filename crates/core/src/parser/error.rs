use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("{}", format_syntax_error(.file_path, *line, *column, .message, .suggestion.as_ref(), .source_line.as_ref()))]
    SyntaxError {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
        source_line: Option<String>,
    },

    #[error("{}", format_undefined_reference(.file_path, *line, *column, .reference, .suggestion.as_ref()))]
    UndefinedReference {
        file_path: String,
        line: usize,
        column: usize,
        reference: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_template_variable_mismatch(.file_path, *line, *column, .message, .suggestion.as_ref()))]
    TemplateVariableMismatch {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_file_read_error(.file_path, *line, *column, .path, .source, .suggestion.as_ref()))]
    FileReadError {
        file_path: String,
        line: usize,
        column: usize,
        path: String,
        source: std::io::Error,
        suggestion: Option<String>,
    },

    #[error("Pest parsing error: {0}")]
    PestError(#[from] Box<pest::error::Error<crate::parser::Rule>>),
}

fn format_syntax_error(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: Option<&String>,
    source_line: Option<&String>,
) -> String {
    let mut result = format!("Error: {message}\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(source) = source_line {
        let line_number_width = format!("{line}").len();
        write!(result, "\n{line:>line_number_width$} | {source}").unwrap();

        if column > 0 {
            let caret_position = column - 1;
            let spaces = " ".repeat(caret_position);
            write!(result, "\n{:>width$} | {}^", "", spaces, width = line_number_width).unwrap();
        }
    }

    result.push_str("\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_undefined_reference(
    file_path: &str,
    line: usize,
    column: usize,
    reference: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: undefined reference '{reference}'\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_template_variable_mismatch(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: template variable mismatch: {message}\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_file_read_error(
    file_path: &str,
    line: usize,
    column: usize,
    path: &str,
    source: &std::io::Error,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: file read error: {path} ({source})\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

impl ParserError {
    #[must_use]
    pub const fn syntax_error(
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    ) -> Self {
        Self::SyntaxError {
            file_path,
            line,
            column,
            message,
            suggestion,
            source_line: None,
        }
    }

    #[must_use]
    pub const fn syntax_error_with_source(
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
        source_line: Option<String>,
    ) -> Self {
        Self::SyntaxError {
            file_path,
            line,
            column,
            message,
            suggestion,
            source_line,
        }
    }

    #[must_use]
    pub const fn undefined_reference(
        file_path: String,
        line: usize,
        column: usize,
        reference: String,
        suggestion: Option<String>,
    ) -> Self {
        Self::UndefinedReference {
            file_path,
            line,
            column,
            reference,
            suggestion,
        }
    }

    #[must_use]
    pub const fn template_variable_mismatch(
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    ) -> Self {
        Self::TemplateVariableMismatch {
            file_path,
            line,
            column,
            message,
            suggestion,
        }
    }

    #[must_use]
    pub const fn file_read_error(
        file_path: String,
        line: usize,
        column: usize,
        path: String,
        source: std::io::Error,
        suggestion: Option<String>,
    ) -> Self {
        Self::FileReadError {
            file_path,
            line,
            column,
            path,
            source,
            suggestion,
        }
    }
}
