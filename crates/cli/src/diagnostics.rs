use thiserror::Error;

use crate::app::ExitCode;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("workflow is invalid: {message}")]
    InvalidWorkflow { message: String },

    #[error("internal error: {message}")]
    Internal { message: String },

    #[error("runtime error: {message}")]
    Runtime { message: String },

    #[error("{command_name} is not implemented yet")]
    NotImplemented {
        command_name: &'static str,
        category: NotImplementedCategory,
    },
}

impl CommandError {
    #[must_use]
    pub fn runtime_not_implemented(command_name: &'static str) -> Self {
        Self::NotImplemented {
            command_name,
            category: NotImplementedCategory::Runtime,
        }
    }

    pub fn invalid_workflow(message: impl Into<String>) -> Self {
        Self::InvalidWorkflow { message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal { message: message.into() }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime { message: message.into() }
    }

    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::InvalidWorkflow { .. } => ExitCode::InvalidWorkflow,
            Self::Internal { .. } => ExitCode::InternalError,
            Self::Runtime { .. } => ExitCode::RuntimeFailure,
            Self::NotImplemented { category, .. } => category.exit_code(),
        }
    }

    #[must_use]
    pub fn exit_status_code(&self) -> i32 {
        self.exit_code().code()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotImplementedCategory {
    Runtime,
}

impl NotImplementedCategory {
    #[must_use]
    pub fn exit_code(self) -> ExitCode {
        match self {
            Self::Runtime => ExitCode::RuntimeFailure,
        }
    }
}
