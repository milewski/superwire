use std::fmt::{self, Debug, Display, Formatter};
use superwire_types::ast::{ModelAssetKindSupportError, SourceSpan, Workflow};
use superwire_types::diagnostic::should_render_rich_diagnostics;
use superwire_types::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use thiserror::Error;

#[derive(Error)]
pub enum WorkflowSemanticError {
    #[error("{details}")]
    ParseFailed {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,

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

    #[error("{source}")]
    Spanned {
        source: Box<WorkflowSemanticError>,
        span: SourceSpan,
    },

    #[error("{message}")]
    Other { message: String },
}

impl From<ModelAssetKindSupportError> for WorkflowSemanticError {
    fn from(error: ModelAssetKindSupportError) -> Self {
        Self::Other { message: error.message }
    }
}

impl Debug for WorkflowSemanticError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, formatter)
    }
}

impl WorkflowSemanticError {
    #[must_use]
    pub fn with_span(self, span: SourceSpan) -> Self {
        if matches!(self, Self::Spanned { .. }) {
            return self;
        }

        Self::Spanned {
            source: Box::new(self),
            span,
        }
    }

    #[must_use]
    pub fn into_compilation_diagnostic(self, workflow: &Workflow, source_name: &str) -> Self {
        match self {
            Self::ParseFailed { source, details } => Self::ParseFailed { source, details },
            Self::InvalidWorkflow { issues } => Self::InvalidWorkflow { issues },
            non_rendered_error => {
                let rendered_error = if let Some(source_text) = workflow.source_text() {
                    if should_render_rich_diagnostics() {
                        non_rendered_error.diagnostic().render_with_source(source_text, source_name)
                    } else {
                        non_rendered_error.diagnostic().render()
                    }
                } else {
                    non_rendered_error.diagnostic().render()
                };

                Self::InvalidWorkflow { issues: rendered_error }
            }
        }
    }

    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let (semantic_error, primary_span) = self.semantic_error_and_span();
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::WorkflowCompilationError,
            DiagnosticSeverity::Error,
            semantic_error.compilation_message(),
            primary_span,
        );

        if let Some(help_message) = semantic_error.compilation_help() {
            diagnostic = diagnostic.with_help(help_message);
        }

        diagnostic
    }

    fn semantic_error_and_span(&self) -> (&Self, Option<SourceSpan>) {
        match self {
            Self::Spanned { source, span } => {
                let (semantic_error, nested_span) = source.semantic_error_and_span();

                (semantic_error, nested_span.or(Some(*span)))
            }
            semantic_error => (semantic_error, None),
        }
    }

    fn compilation_message(&self) -> String {
        match self {
            Self::ParseFailed { source: _, details } => details.clone(),
            Self::InvalidWorkflow { issues } => issues.clone(),
            Self::ExecutionPlanInvariant { message } => {
                format!("Execution plan invariant violated: {message}")
            }
            Self::MissingDeclaration { message } => message.clone(),
            Self::UnsupportedFeature { feature } => {
                format!("Unsupported workflow feature: {feature}")
            }
            Self::ProviderConfiguration { provider_name, message } => {
                format!("Provider `{provider_name}` configuration error: {message}")
            }
            Self::ExpressionEvaluation { context, message } => {
                format!("Failed to evaluate expression in {context}: {message}")
            }
            Self::InvalidAgentProperty {
                agent_name,
                property,
                message,
            } => {
                format!("Agent `{agent_name}` has invalid `{property}` property: {message}")
            }
            Self::InputTypeMismatch { expected, found } => {
                format!("Workflow input type mismatch: expected `{expected}`, found `{found}`")
            }
            Self::OutputTypeMismatch { expected, found } => {
                format!("Workflow output type mismatch: expected `{expected}`, found `{found}`")
            }
            Self::InputValueMismatch { message } => {
                format!("Workflow input value does not match declared input type: {message}")
            }
            Self::AgentOutputTypeMismatch { agent_name, message } => {
                format!("Agent `{agent_name}` output does not match declared output type: {message}")
            }
            Self::SerializationFailed { context, source } => {
                format!("Failed to serialize {context}: {source}")
            }
            Self::OutputDeserializationFailed { source } => {
                format!("Failed to deserialize workflow output: {source}")
            }
            Self::Spanned { source, span: _ } => source.compilation_message(),
            Self::Other { message } => message.clone(),
        }
    }

    fn compilation_help(&self) -> Option<String> {
        match self {
            Self::ParseFailed { source: _, details: _ } | Self::InvalidWorkflow { issues: _ } => None,
            Self::MissingDeclaration { message } => {
                if message.contains("`output` block") {
                    return Some("Add an `output { ... }` declaration at the workflow root.".to_string());
                }

                Some("Add the missing declaration required by the workflow compiler.".to_string())
            }
            Self::InvalidAgentProperty {
                agent_name,
                property,
                message: _,
            } => Some(format!(
                "Set `{property}` on `agent {agent_name}` with a valid value that matches DSL requirements."
            )),
            Self::InputTypeMismatch { expected, found: _ } => Some(format!(
                "Update Rust input schema or DSL `input` declaration so the expected type is `{expected}`."
            )),
            Self::OutputTypeMismatch { expected, found: _ } => Some(format!(
                "Update Rust output schema or workflow `output` fields so the expected type is `{expected}`."
            )),
            Self::ExecutionPlanInvariant { message: _ } => {
                Some("Check agent dependencies and provider declarations for planner consistency.".to_string())
            }
            Self::UnsupportedFeature { feature: _ } => {
                Some("Remove the unsupported feature or implement support before running this workflow.".to_string())
            }
            Self::ProviderConfiguration {
                provider_name: _,
                message: _,
            } => Some("Fix provider configuration fields so the provider can be initialized.".to_string()),
            Self::ExpressionEvaluation { context: _, message: _ } => Some(
                "Compare the reported expected and found types, then update the expression, field path, or declaration so every branch is compatible."
                    .to_string(),
            ),
            Self::InputValueMismatch { message: _ } => Some("Pass input data that matches the declared workflow `input` type.".to_string()),
            Self::AgentOutputTypeMismatch { agent_name: _, message: _ } => {
                Some("Make sure the agent response matches its declared `output` type.".to_string())
            }
            Self::Spanned { source, span: _ } => source.compilation_help(),
            Self::SerializationFailed { context: _, source: _ }
            | Self::OutputDeserializationFailed { source: _ }
            | Self::Other { message: _ } => {
                Some("Review the diagnostic message and adjust workflow/runtime configuration accordingly.".to_string())
            }
        }
    }
}
