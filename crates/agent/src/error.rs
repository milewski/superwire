use std::collections::HashMap;
use thiserror::Error;



/// Agent execution error
#[derive(Debug, Clone, Error)]
pub enum AgentError {
    #[error("Maximum retries ({max_retries}) exceeded")]
    MaxRetriesExceeded { max_retries: usize },

    #[error("Maximum tokens ({max_tokens}) exceeded; used {used_tokens}")]
    MaxTokensExceeded { max_tokens: usize, used_tokens: usize },

    #[error(transparent)]
    ExecutionFailed(#[from] ExecutorError),
}

/// Executor error with structured details
#[derive(Debug, Clone, Error)]
pub enum ExecutorError {
    /// Returned when the executor reaches its maximum iteration budget
    /// before a finalize tool call completes the task.
    #[error("Maximum iterations ({max_iterations}) reached without calling finalize tool")]
    MaxIterationsReached { max_iterations: usize },

    /// Returned when the provider call fails during a specific loop iteration.
    #[error("Provider error at iteration {iteration}: {message}")]
    ProviderFailed { iteration: usize, message: String },

    /// Returned when repeated conversation content indicates the agent is stuck.
    #[error("Agent is stuck in a repeated loop")]
    StuckLoopDetected,

    /// Returned when a successful finalize payload cannot be serialized to JSON.
    #[error("Failed to serialize finalize tool output: {message}")]
    FinalizeOutputSerializationFailed { message: String },

    /// Returned when the finalize tool explicitly reports a task failure reason.
    #[error("Agent failed to complete the task: {reason}")]
    FinalizeFailure { reason: String },

    /// Returned when a runtime tool error is converted into an executor error.
    #[error("{message}")]
    ToolError {
        message: String,
        details: HashMap<String, serde_json::Value>,
    },
}

impl From<crate::tool::ToolError> for ExecutorError {
    fn from(error: crate::tool::ToolError) -> Self {
        Self::ToolError {
            message: error.error,
            details: error.context,
        }
    }
}

impl From<crate::tool::ToolError> for AgentError {
    fn from(error: crate::tool::ToolError) -> Self {
        Self::ExecutionFailed(ExecutorError::from(error))
    }
}
