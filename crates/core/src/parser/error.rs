use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("Syntax error at line {line}, column {column}: {message}")]
    SyntaxError {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Undefined reference at line {line}, column {column}: {reference}")]
    UndefinedReference {
        file_path: String,
        line: usize,
        column: usize,
        reference: String,
        suggestion: Option<String>,
    },

    #[error("Template variable mismatch at line {line}, column {column}: {message}")]
    TemplateVariableMismatch {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("File read error at line {line}, column {column}: {path}")]
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
