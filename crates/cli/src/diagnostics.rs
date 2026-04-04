use thiserror::Error;

use crate::app::ExitCode;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("{message}")]
    InvalidInput { message: String },

    #[error("{message}")]
    Internal { message: String },
}

impl CommandError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput { message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal { message: message.into() }
    }

    pub const fn exit_code(&self) -> ExitCode {
        match self {
            Self::InvalidInput { message: _ } => ExitCode::InvalidInput,
            Self::Internal { message: _ } => ExitCode::InternalError,
        }
    }
}
