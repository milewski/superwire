use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("Schema compilation error: {message}")]
    CompilationError {
        schema_name: Option<String>,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Schema validation error: {message}")]
    ValidationError {
        schema_name: Option<String>,
        field_path: Option<String>,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Unsupported schema type: {type_name}")]
    UnsupportedType {
        type_name: String,
        suggestion: Option<String>,
    },
}
