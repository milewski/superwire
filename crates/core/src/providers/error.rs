use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Connection error: {message}")]
    ConnectionError { message: String, suggestion: Option<String> },

    #[error("Model not found: {model}")]
    ModelNotFound {
        model: String,
        available_models: Vec<String>,
        suggestion: Option<String>,
    },

    #[error("API error: {message}")]
    ApiError {
        message: String,
        status_code: Option<u16>,
        suggestion: Option<String>,
    },

    #[error("Response parsing error: {message}")]
    ResponseParsingError { message: String, suggestion: Option<String> },

    #[error("Tool call error: {message}")]
    ToolCallError { message: String, suggestion: Option<String> },

    #[error("Execution error: {message}")]
    ExecutionError { message: String },

    #[error("Invalid input: {message}")]
    InvalidInput { message: String },
}
