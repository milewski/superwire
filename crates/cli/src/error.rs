use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Workflow file not found: {0}")]
    WorkflowNotFound(String),

    #[error("Execution error: {0}")]
    ExecutionError(#[from] engine_ai_core::execution::error::ExecutionError),

    #[error("Failed to serialize output: {0}")]
    OutputSerializationError(#[from] serde_json::Error),

    #[error("Compilation error: {0}")]
    CompilationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),

    #[error("Validation error: {0}")]
    ValidationError(#[from] engine_ai_core::validation::error::ValidationError),

    #[error("Parse error: {0}")]
    ParseError(#[from] engine_ai_core::parser::error::ParserError),
}
