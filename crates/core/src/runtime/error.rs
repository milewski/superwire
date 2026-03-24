use crate::dsl::{DslParseError, SourceSpan, ValidationIssue};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationProblem {
    pub issue: ValidationIssue,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Error)]
pub enum WorkflowRuntimeError {
    #[error("failed to parse workflow: {0}")]
    Parse(#[from] DslParseError),

    #[error("workflow validation failed")]
    ValidationFailed { problems: Vec<ValidationProblem> },

    #[error("agent '{agent_name}' is missing a model property")]
    MissingModelExpression { agent_name: String },

    #[error("agent '{agent_name}' has an invalid model property")]
    InvalidModelExpression { agent_name: String },

    #[error("agent '{agent_name}' references unknown provider '{provider_name}'")]
    MissingProviderDeclaration { agent_name: String, provider_name: String },

    #[error("agent '{agent_name}' uses for-loop execution which is not supported by this runtime yet")]
    UnsupportedForLoop { agent_name: String },

    #[error("tool keyword references are not supported in {context}")]
    UnsupportedToolKeywordReference { context: String },

    #[error("function '{function_name}' is not supported in {context}")]
    UnsupportedFunctionCall { function_name: String, context: String },

    #[error("unknown reference identifier '{identifier}' in {context}")]
    UnknownReferenceIdentifier { identifier: String, context: String },

    #[error("invalid reference '{reference_path}': cannot access field '{field_name}' in {context}")]
    InvalidReferencePath {
        reference_path: String,
        field_name: String,
        context: String,
    },

    #[error("provider factory failed: {message}")]
    ProviderFactoryFailed { message: String },

    #[error("failed to construct loop executor for agent '{agent_name}': {message}")]
    LoopExecutorCreationFailed { agent_name: String, message: String },

    #[error("agent '{agent_name}' execution failed: {message}")]
    AgentExecutionFailed { agent_name: String, message: String },

    #[error("agent '{agent_name}' output type mismatch: {message}")]
    AgentOutputTypeMismatch { agent_name: String, message: String },

    #[error("agent dependency cycle detected: {agent_names:?}")]
    DependencyCycle { agent_names: Vec<String> },

    #[error("invalid numeric literal '{literal}' in {context}")]
    InvalidNumberLiteral { literal: String, context: String },
}
