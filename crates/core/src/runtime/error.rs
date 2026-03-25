use crate::diagnostics::{diagnostic_from_parse_error, render_diagnostics, Diagnostic, DiagnosticCode};
use crate::dsl::DslParseError;
use engine_ai_agent::AgentError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowRuntimeError {
    #[error("workflow parse failed: {source}")]
    ParseFailed {
        #[source]
        source: DslParseError,
    },

    #[error("workflow validation failed:\n{rendered}")]
    InvalidWorkflow { diagnostics: Vec<Diagnostic>, rendered: String },

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

impl WorkflowRuntimeError {
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        match self {
            Self::ParseFailed { source } => vec![diagnostic_from_parse_error(source)],
            Self::InvalidWorkflow { diagnostics, rendered: _ } => diagnostics.clone(),
            Self::ExecutionPlanInvariant { message } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeExecutionPlanInvariant,
                format!("execution plan invariant violation: {message}"),
            )],
            Self::MissingDeclaration { message } => {
                vec![Diagnostic::error(DiagnosticCode::RuntimeMissingDeclaration, message.clone())]
            }
            Self::UnsupportedFeature { feature } => {
                vec![Diagnostic::error(DiagnosticCode::RuntimeUnsupportedFeature, feature.clone())]
            }
            Self::ProviderConfiguration { provider_name, message } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeProviderConfiguration,
                format!("provider `{provider_name}` configuration error: {message}"),
            )],
            Self::ExpressionEvaluation { context, message } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeExpressionEvaluation,
                format!("failed to evaluate expression in {context}: {message}"),
            )],
            Self::InvalidAgentProperty {
                agent_name,
                property,
                message,
            } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeInvalidAgentProperty,
                format!("agent `{agent_name}` has invalid `{property}` property: {message}"),
            )],
            Self::InputTypeMismatch { expected, found } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeInputTypeMismatch,
                format!("workflow input type mismatch: expected `{expected}`, found `{found}`"),
            )],
            Self::OutputTypeMismatch { expected, found } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeOutputTypeMismatch,
                format!("workflow output type mismatch: expected `{expected}`, found `{found}`"),
            )],
            Self::InputValueMismatch { message } => {
                vec![Diagnostic::error(DiagnosticCode::RuntimeInputValueMismatch, message.clone())]
            }
            Self::AgentOutputTypeMismatch { agent_name, message } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeAgentOutputTypeMismatch,
                format!("agent `{agent_name}` output type mismatch: {message}"),
            )],
            Self::AgentExecutionFailed { agent_name, source } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeAgentExecutionFailed,
                format!("agent execution failed for `{agent_name}`: {source}"),
            )],
            Self::SerializationFailed { context, source } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeSerializationFailed,
                format!("failed to serialize {context}: {source}"),
            )],
            Self::OutputDeserializationFailed { source } => vec![Diagnostic::error(
                DiagnosticCode::RuntimeOutputDeserializationFailed,
                format!("failed to deserialize workflow output: {source}"),
            )],
            Self::Other { message } => vec![Diagnostic::error(DiagnosticCode::RuntimeOther, message.clone())],
        }
    }

    #[must_use]
    pub fn rendered_diagnostics(&self, source_code: Option<&str>) -> String {
        if let Self::InvalidWorkflow { diagnostics: _, rendered } = self {
            if source_code.is_none() {
                return rendered.clone();
            }
        }

        render_diagnostics(&self.diagnostics(), source_code)
    }
}

#[cfg(test)]
mod tests {
    use super::WorkflowRuntimeError;
    use crate::diagnostics::DiagnosticCode;
    use crate::dsl::parse_workflow;

    #[test]
    fn maps_parse_error_variant_to_parser_diagnostic() {
        let parse_error =
            parse_workflow("agent a {\n  prompt: \"hello\"\n}\n@\n").expect_err("workflow should fail to parse for invalid token");

        let runtime_error = WorkflowRuntimeError::ParseFailed { source: parse_error };
        let diagnostics = runtime_error.diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ParsePest);
        assert!(diagnostics[0].primary_span.is_some());
    }

    #[test]
    fn maps_runtime_error_variants_to_runtime_diagnostics() {
        let runtime_error = WorkflowRuntimeError::InputTypeMismatch {
            expected: "{ topic: string }".to_string(),
            found: "null".to_string(),
        };

        let diagnostics = runtime_error.diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::RuntimeInputTypeMismatch);
    }
}
