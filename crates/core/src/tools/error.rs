use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("invalid tool input: {message}")]
    InvalidInput { message: String },
    #[error("tool implementation is not available yet")]
    Unimplemented,
}
