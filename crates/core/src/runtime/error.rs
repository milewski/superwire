use crate::dsl::DslParseError;
use engine_ai_agent::AgentError;
use std::fmt::{self, Debug, Display, Formatter};
use thiserror::Error;

#[derive(Error)]
pub enum WorkflowRuntimeError {
    #[error("{details}")]
    ParseFailed {
        #[source]
        source: DslParseError,

        details: String,
    },

    #[error("{issues}")]
    InvalidWorkflow { issues: String },

    #[error("execution plan invariant violated: {message}")]
    ExecutionPlanInvariant { message: String },

    #[error("missing declaration: {message}")]
    MissingDeclaration { message: String },

    #[error("unsupported workflow feature: {feature}")]
    UnsupportedFeature { feature: String },

    #[error("provider `{provider_name}` configuration error: {message}")]
    ProviderConfiguration { provider_name: String, message: String },

    #[error("failed to evaluate expression in {context}: {message}")]
    ExpressionEvaluation { context: String, message: String },

    #[error("agent `{agent_name}` has invalid `{property}` property: {message}")]
    InvalidAgentProperty {
        agent_name: String,
        property: String,
        message: String,
    },

    #[error("workflow input type mismatch: expected `{expected}`, found `{found}`")]
    InputTypeMismatch { expected: String, found: String },

    #[error("workflow output type mismatch: expected `{expected}`, found `{found}`")]
    OutputTypeMismatch { expected: String, found: String },

    #[error("workflow input value does not match declared input type: {message}")]
    InputValueMismatch { message: String },

    #[error("agent `{agent_name}` output does not match declared output type: {message}")]
    AgentOutputTypeMismatch { agent_name: String, message: String },

    #[error("agent execution failed for `{agent_name}`: {source}")]
    AgentExecutionFailed {
        agent_name: String,
        #[source]
        source: Box<AgentError>,
    },

    #[error("failed to serialize {context}: {source}")]
    SerializationFailed {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to deserialize workflow output: {source}")]
    OutputDeserializationFailed {
        #[source]
        source: serde_json::Error,
    },

    #[error("{message}")]
    Other { message: String },
}

impl Debug for WorkflowRuntimeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, formatter)
    }
}
