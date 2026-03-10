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

/// Simple error type for use within tool implementations.
/// Automatically converted to `ToolError` with the tool name filled in.
#[derive(Debug)]
pub struct SimpleToolError {
    pub message: String,
    pub suggestion: Option<String>,
}

impl SimpleToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            suggestion: None,
        }
    }

    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    #[must_use]
    pub fn into_tool_error(self, tool_name: String) -> ToolError {
        ToolError::ExecutionError {
            tool_name,
            message: self.message,
            suggestion: self.suggestion,
        }
    }
}

/// Convenience macro for creating simple tool errors
#[macro_export]
macro_rules! tool_error {
    ($msg:expr) => {
        $crate::tools::error::SimpleToolError::new($msg)
    };
    ($msg:expr, $suggestion:expr) => {
        $crate::tools::error::SimpleToolError::new($msg).with_suggestion($suggestion)
    };
}
