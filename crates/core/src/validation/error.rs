use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Duplicate name at line {line}, column {column}: {name}")]
    DuplicateName {
        file_path: String,
        line: usize,
        column: usize,
        name: String,
        first_defined_at: String,
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

    #[error("Provider/model mismatch at line {line}, column {column}: {message}")]
    ProviderModelMismatch {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Missing template variable at line {line}, column {column}: {variable}")]
    MissingTemplateVariable {
        file_path: String,
        line: usize,
        column: usize,
        variable: String,
        suggestion: Option<String>,
    },

    #[error("Unused template binding at line {line}, column {column}: {binding}")]
    UnusedTemplateBinding {
        file_path: String,
        line: usize,
        column: usize,
        binding: String,
        suggestion: Option<String>,
    },

    #[error("Invalid property at line {line}, column {column}: {property}")]
    InvalidProperty {
        file_path: String,
        line: usize,
        column: usize,
        property: String,
        suggestion: Option<String>,
    },

    #[error("Cyclic dependency detected: {cycle}")]
    CyclicDependency {
        file_path: String,
        cycle: String,
        suggestion: Option<String>,
    },

    #[error("Invalid input/output block at line {line}, column {column}: {message}")]
    InvalidInputOutput {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },
}
