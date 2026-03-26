use thiserror::Error;

use crate::app::ExitCode;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("workflow is invalid: {message}")]
    InvalidWorkflow { message: String },

    #[error("{command_name} is not implemented yet")]
    NotImplemented {
        command_name: &'static str,
        category: NotImplementedCategory,
    },
}

impl CommandError {
    pub fn runtime_not_implemented(command_name: &'static str) -> Self {
        Self::NotImplemented {
            command_name,
            category: NotImplementedCategory::Runtime,
        }
    }

    pub fn internal_not_implemented(command_name: &'static str) -> Self {
        Self::NotImplemented {
            command_name,
            category: NotImplementedCategory::Internal,
        }
    }

    pub fn invalid_workflow(message: impl Into<String>) -> Self {
        Self::InvalidWorkflow { message: message.into() }
    }

    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::InvalidWorkflow { .. } => ExitCode::InvalidWorkflow,
            Self::NotImplemented { category, .. } => category.exit_code(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotImplementedCategory {
    Runtime,
    Internal,
}

impl NotImplementedCategory {
    pub fn exit_code(self) -> ExitCode {
        match self {
            Self::Runtime => ExitCode::RuntimeFailure,
            Self::Internal => ExitCode::InternalError,
        }
    }
}
