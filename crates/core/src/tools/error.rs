use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Tool execution error: {message}")]
    ExecutionError {
        tool_name: String,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Invalid tool parameters: {message}")]
    InvalidParameters {
        tool_name: String,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Tool not found: {tool_name}")]
    ToolNotFound {
        tool_name: String,
        available_tools: Vec<String>,
        suggestion: Option<String>,
    },
}
