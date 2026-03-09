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
}
