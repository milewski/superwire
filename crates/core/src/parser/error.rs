use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("{}", format_syntax_error(.file_path, *line, *column, .message, .suggestion, .source_line))]
    SyntaxError {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
        source_line: Option<String>,
    },

    #[error("{}", format_undefined_reference(.file_path, *line, *column, .reference, .suggestion))]
    UndefinedReference {
        file_path: String,
        line: usize,
        column: usize,
        reference: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_template_variable_mismatch(.file_path, *line, *column, .message, .suggestion))]
    TemplateVariableMismatch {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_file_read_error(.file_path, *line, *column, .path, .source, .suggestion))]
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
    suggestion: &Option<String>,
    source_line: &Option<String>,
) -> String {
    let mut result = format!("Error: {}\n  --> {}:{}:{}\n   |", message, file_path, line, column);

    if let Some(source) = source_line {
        let line_number_width = format!("{}", line).len();
        result.push_str(&format!("\n{:>width$} | {}", line, source, width = line_number_width));

        if column > 0 {
            let caret_position = column - 1;
            let spaces = " ".repeat(caret_position);
            result.push_str(&format!("\n{:>width$} | {}^", "", spaces, width = line_number_width));
        }
    }

    result.push_str("\n   |");

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_undefined_reference(
    file_path: &str,
    line: usize,
    column: usize,
    reference: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: undefined reference '{}'\n  --> {}:{}:{}\n   |",
        reference, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_template_variable_mismatch(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: template variable mismatch: {}\n  --> {}:{}:{}\n   |",
        message, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_file_read_error(
    file_path: &str,
    line: usize,
    column: usize,
    path: &str,
    source: &std::io::Error,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: file read error: {} ({})\n  --> {}:{}:{}\n   |",
        path, source, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

impl ParserError {
    pub fn syntax_error(
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

    pub fn syntax_error_with_source(
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

    pub fn undefined_reference(
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

    pub fn template_variable_mismatch(
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

    pub fn file_read_error(
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
