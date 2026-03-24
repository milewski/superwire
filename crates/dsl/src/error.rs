use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum WorkflowError {
    #[error("Execution error: {message}")]
    Execution { message: String },

    #[error("I/O error: {message}")]
    Io { message: String },

    #[error("Parse error: {message}")]
    Parse { message: String },

    #[error("Schema error: {message}")]
    Schema { message: String },

    #[error("Validation error: {message}")]
    Validation { message: String },
}

impl WorkflowError {
    #[must_use]
    pub fn execution(message: impl Into<String>) -> Self {
        Self::Execution { message: message.into() }
    }

    #[must_use]
    pub fn io(message: impl Into<String>) -> Self {
        Self::Io { message: message.into() }
    }

    #[must_use]
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse { message: message.into() }
    }

    #[must_use]
    pub fn schema(message: impl Into<String>) -> Self {
        Self::Schema { message: message.into() }
    }

    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation { message: message.into() }
    }
}

impl From<std::io::Error> for WorkflowError {
    fn from(error: std::io::Error) -> Self {
        Self::io(error.to_string())
    }
}
