use crate::context::Context;
use std::collections::HashMap;
use thiserror::Error;

/// Agent execution error
#[derive(Debug, Clone, Error)]
pub enum AgentError {
    #[error("Maximum retries ({max_retries}) exceeded")]
    MaxRetriesExceeded { max_retries: usize },

    #[error("Maximum tokens ({max_tokens}) exceeded; used {used_tokens}")]
    MaxTokensExceeded { max_tokens: usize, used_tokens: usize },

    #[error("{error}. Context: {context:?}")]
    ExecutionFailed { error: ExecutorError, context: Context },
}

/// Executor error with structured details
#[derive(Debug, Clone, Error)]
pub enum ExecutorError {
    /// Returned when the executor reaches its maximum iteration budget
    /// before a completion tool call completes the task.
    #[error("Maximum iterations ({max_iterations}) reached without calling a completion tool")]
    MaxIterationsReached { max_iterations: usize },

    /// Returned when the provider call fails.
    #[error("Provider error: {error}")]
    ProviderFailed { error: ProviderError },

    /// Returned when repeated conversation content indicates the agent is stuck.
    #[error("Agent is stuck in a repeated loop")]
    StuckLoopDetected,

    /// Returned when a provider ignores required tool-call mode and returns no tools.
    #[error("Provider ignored required tool choice and returned no tool calls")]
    ProviderIgnoredRequiredToolChoice,

    /// Returned when the provider stops due to token limit.
    #[error("Provider reached maximum token limit")]
    MaxTokensReached,

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

/// Structured provider error with categorized failure reasons.
#[derive(Debug, Clone, Error)]
pub enum ProviderError {
    /// Provider rejected request due to authentication/authorization failure.
    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },

    /// Provider rejected request due to invalid request payload.
    #[error("Invalid provider request: {message}")]
    InvalidRequest { message: String },

    /// Provider is currently rate limiting requests.
    #[error("Rate limited by provider: {message}")]
    RateLimited { message: String, retry_after_seconds: Option<u64> },

    /// Provider service is temporarily unavailable.
    #[error("Provider service unavailable: {message}")]
    ServiceUnavailable { message: String },

    /// Network-level issue while contacting provider.
    #[error("Provider network error: {message}")]
    Network { message: String },

    /// Provider response could not be parsed or interpreted.
    #[error("Provider response parse error: {message}")]
    ResponseParseFailed { message: String },

    /// Other provider failure not captured by a specific variant.
    #[error("Provider error: {message}")]
    Other { message: String },
}

impl ProviderError {
    #[must_use]
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited {
                message: _,
                retry_after_seconds: _,
            } | Self::ServiceUnavailable { message: _ }
                | Self::Network { message: _ }
        )
    }
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
        Self::ExecutionFailed {
            error: ExecutorError::from(error),
            context: Context::default(),
        }
    }
}
