use std::collections::HashMap;
use thiserror::Error;

/// Validation error containing details about why validation failed
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct ValidationError {
    pub message: String,
    pub details: HashMap<String, serde_json::Value>,
}

impl ValidationError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }

    #[must_use]
    pub fn get_detail(&self, key: &str) -> Option<&serde_json::Value> {
        self.details.get(key)
    }
}

/// Agent execution error
#[derive(Debug, Clone, Error)]
pub enum AgentError {
    #[error("Maximum retries ({max_retries}) exceeded")]
    MaxRetriesExceeded { max_retries: usize },

    #[error("Maximum tokens ({max_tokens}) exceeded; used {used_tokens}")]
    MaxTokensExceeded { max_tokens: usize, used_tokens: usize },

    #[error("Validation failed: {error}")]
    ValidationFailed { error: ValidationError },

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolError;
    use serde_json::json;

    #[test]
    fn test_validation_error_creation() {
        let error = ValidationError::new("Test error")
            .with_detail("field", json!("value"))
            .with_detail("code", json!(42));

        assert_eq!(error.message, "Test error");
        assert_eq!(error.get_detail("field"), Some(&json!("value")));
        assert_eq!(error.get_detail("code"), Some(&json!(42)));
        assert_eq!(error.get_detail("missing"), None);
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::new("Display test");
        assert_eq!(format!("{}", error), "Display test");
    }

    #[test]
    fn test_tool_error_converts_to_execution_failed_agent_error() {
        let agent_error: AgentError = ToolError::new("Tool failed").into();

        match agent_error {
            AgentError::ExecutionFailed(ExecutorError::ToolError { message, .. }) => {
                assert_eq!(message, "Tool failed")
            }
            _ => panic!("expected execution failed error"),
        }
    }
}
