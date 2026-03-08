use thiserror::Error;

use crate::providers::error::ProviderError;
use crate::schemas::error::SchemaError;
use crate::tools::error::ToolError;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("schema error: {0}")]
    Schema(#[from] SchemaError),
    #[error("tool error: {0}")]
    Tool(#[from] ToolError),
    #[error("agent `{agent}` did not call done")]
    MissingDoneCall { agent: String },
    #[error("agent `{agent}` returned invalid done payload: {message}")]
    InvalidDonePayload { agent: String, message: String },
    #[error("dependency cycle detected at agent `{node}`")]
    DependencyCycle { node: String },
    #[error("missing execution result for agent `{agent}`")]
    MissingAgentResult { agent: String },
    #[error("schema `{schema}` referenced by agent `{agent}` not found")]
    MissingSchema { agent: String, schema: String },
    #[error("schema compilation failed for agent `{agent}`: {message}")]
    SchemaCompilation { agent: String, message: String },
    #[error("for_each requires an array collection, got `{actual}`")]
    InvalidForEachCollection { actual: String },
    #[error("unsupported expression in runtime conversion: `{expression}`")]
    UnsupportedExpression { expression: String },
    #[error("invalid numeric value `{value}`")]
    InvalidNumericValue { value: String },
    #[error("invalid context reference `{reference}`")]
    InvalidContextReference { reference: String },
    #[error("agent `{agent}` is missing a model configuration")]
    MissingModel { agent: String },
    #[error("provider definition `{provider}` is missing")]
    MissingProviderDefinition { provider: String },
    #[error("execution implementation is not available yet")]
    Unimplemented,
}
