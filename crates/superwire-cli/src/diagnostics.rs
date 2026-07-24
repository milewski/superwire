use serde_json::Value;
use thiserror::Error;

use crate::app::ExitCode;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("{message}")]
    InvalidInput { message: String, details: Option<Value> },

    #[error("{message}")]
    Internal { message: String, details: Option<Value> },
}

impl CommandError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
            details: None,
        }
    }

    pub fn invalid_input_with_details(message: impl Into<String>, details: Value) -> Self {
        Self::InvalidInput {
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            details: None,
        }
    }

    pub fn internal_with_details(message: impl Into<String>, details: Value) -> Self {
        Self::Internal {
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput { message: _, details: _ } => "invalid_input",
            Self::Internal { message: _, details: _ } => "internal_error",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::InvalidInput { message, details: _ } => message,
            Self::Internal { message, details: _ } => message,
        }
    }

    pub fn details(&self) -> Option<&Value> {
        match self {
            Self::InvalidInput { message: _, details } | Self::Internal { message: _, details } => details.as_ref(),
        }
    }

    pub const fn exit_code(&self) -> ExitCode {
        match self {
            Self::InvalidInput { message: _, details: _ } => ExitCode::InvalidInput,
            Self::Internal { message: _, details: _ } => ExitCode::InternalError,
        }
    }
}
