use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Runtime error in agent {agent}: {message}")]
    RuntimeError {
        agent: String,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Tool execution error in agent {agent}: {tool_name}")]
    ToolExecutionError {
        agent: String,
        tool_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
        suggestion: Option<String>,
    },

    #[error("Provider error in agent {agent}: {message}")]
    ProviderError {
        agent: String,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Schema validation error in agent {agent}: {message}")]
    SchemaValidationError {
        agent: String,
        message: String,
        field_path: Option<String>,
        suggestion: Option<String>,
    },

    #[error("Agent failed with status 'fail': {reason}")]
    AgentFailed {
        agent: String,
        reason: String,
        suggestion: Option<String>,
    },

    #[error("Context operation error: {message}")]
    ContextError {
        message: String,
        suggestion: Option<String>,
    },
}
