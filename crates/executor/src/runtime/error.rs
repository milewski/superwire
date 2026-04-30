use superwire_core::semantic::WorkflowSemanticError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error(transparent)]
    Semantic(#[from] WorkflowSemanticError),

    #[error("workflow input type mismatch: expected `{expected}`, found `{found}`")]
    InputTypeMismatch { expected: String, found: String },

    #[error("workflow input value does not match declared input type: {message}")]
    InputValueMismatch { message: String },

    #[error("workflow output type mismatch: expected `{expected}`, found `{found}`")]
    OutputTypeMismatch { expected: String, found: String },

    #[error("agent `{agent_name}` output does not match declared output type: {message}")]
    AgentOutputTypeMismatch { agent_name: String, message: String },

    #[error("agent execution failed for `{agent_name}`: {message}")]
    Model { agent_name: String, message: String },

    #[error("{message}")]
    Other { message: String },
}

impl ExecutorError {
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        match self {
            Self::Semantic(WorkflowSemanticError::ParseFailed { .. } | WorkflowSemanticError::InvalidWorkflow { .. })
            | Self::InputTypeMismatch { .. }
            | Self::InputValueMismatch { .. } => true,
            Self::Semantic(_)
            | Self::OutputTypeMismatch { .. }
            | Self::AgentOutputTypeMismatch { .. }
            | Self::Model { .. }
            | Self::Other { .. } => false,
        }
    }
}
