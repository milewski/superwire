use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("duplicate agent name `{name}`")]
    DuplicateAgent { name: String },
    #[error("duplicate schema name `{name}`")]
    DuplicateSchema { name: String },
    #[error("duplicate provider name `{name}`")]
    DuplicateProvider { name: String },
    #[error("duplicate workflow input block")]
    DuplicateWorkflowInput,
    #[error("duplicate workflow output block")]
    DuplicateWorkflowOutput,
    #[error("agent `{agent}` references undefined provider `{provider}`")]
    UndefinedProvider { agent: String, provider: String },
    #[error("agent `{agent}` references model `{model}` not declared by provider `{provider}`")]
    ProviderModelMismatch {
        agent: String,
        provider: String,
        model: String,
    },
    #[error("{scope} uses invalid property `{property}`")]
    InvalidProperty { scope: String, property: String },
    #[error("agent `{agent}` references undefined schema `{schema}`")]
    UndefinedSchema { agent: String, schema: String },
    #[error("{scope} references undefined agent path `{reference}`")]
    UndefinedAgent { scope: String, reference: String },
    #[error("{scope} has invalid reference `{reference}`: {message}")]
    InvalidReference {
        scope: String,
        reference: String,
        message: String,
    },
    #[error("{scope} has invalid function call `{function}`: {message}")]
    InvalidFunctionCall {
        scope: String,
        function: String,
        message: String,
    },
    #[error("cyclic dependency detected in workflow")]
    CyclicDependency,
    #[error("dependency graph error: {message}")]
    DependencyGraph { message: String },
    #[error("validation implementation is not available yet")]
    Unimplemented,
}
