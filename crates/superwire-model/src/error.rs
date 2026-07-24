use std::error::Error;
use superwire_mcp::McpError;
use superwire_protocol::event::{
    DiagnosticRetryability, ExecutorDiagnostic, ExecutorDiagnosticCode, ExecutorDiagnosticSubject, ExecutorStage,
};
use thiserror::Error;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Error)]
#[error("{diagnostic}")]
pub struct ModelProviderError {
    diagnostic: Box<ExecutorDiagnostic>,

    #[source]
    source: Option<BoxError>,
}

impl ModelProviderError {
    #[must_use]
    pub fn model(agent_name: impl Into<String>, message: impl Into<String>) -> Self {
        let agent_name = agent_name.into();
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ModelProviderFailed,
            ExecutorStage::Model,
            message,
            Self::provider_subject(agent_name),
        )
        .with_retryability(DiagnosticRetryability::Unknown);

        Self::from_diagnostic(diagnostic)
    }

    #[must_use]
    pub fn model_with_source<ErrorType>(agent_name: impl Into<String>, message: impl Into<String>, source: ErrorType) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        let agent_name = agent_name.into();
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ModelProviderFailed,
            ExecutorStage::Model,
            message,
            Self::provider_subject(agent_name),
        )
        .with_retryability(DiagnosticRetryability::Unknown);

        Self::with_source(diagnostic, source)
    }

    #[must_use]
    pub fn rejected(agent_name: impl Into<String>, message: impl Into<String>) -> Self {
        let agent_name = agent_name.into();
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ModelRejected,
            ExecutorStage::Model,
            message,
            Self::provider_subject(agent_name),
        );

        Self::from_diagnostic(diagnostic)
    }

    #[must_use]
    pub fn rejected_with_source<ErrorType>(agent_name: impl Into<String>, _message: impl Into<String>, source: ErrorType) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        let agent_name = agent_name.into();
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::ModelRejected,
            ExecutorStage::Model,
            "model response was rejected",
            Self::provider_subject(agent_name),
        );

        Self::with_source(diagnostic, source)
    }

    #[must_use]
    pub fn invalid_output(agent_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::from_diagnostic(ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::InvalidOutput,
            ExecutorStage::Output,
            message,
            ExecutorDiagnosticSubject::Agent {
                agent_name: agent_name.into(),
                iteration_index: None,
            },
        ))
    }

    #[must_use]
    pub fn mcp_with_source<ErrorType>(
        agent_name: String,
        server_name: String,
        target_name: String,
        message: impl Into<String>,
        source: ErrorType,
    ) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::McpFailed,
            ExecutorStage::Mcp,
            message,
            ExecutorDiagnosticSubject::Mcp {
                agent_name: Some(agent_name),
                server_name: Some(server_name),
                target_name: Some(target_name),
            },
        )
        .with_retryability(DiagnosticRetryability::Unknown);

        Self::with_source(diagnostic, source)
    }

    #[must_use]
    pub fn other(message: impl Into<String>) -> Self {
        Self::from_diagnostic(ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::InternalError,
            ExecutorStage::Internal,
            message,
            ExecutorDiagnosticSubject::Workflow,
        ))
    }

    #[must_use]
    pub fn from_diagnostic(diagnostic: ExecutorDiagnostic) -> Self {
        Self {
            diagnostic: Box::new(diagnostic),
            source: None,
        }
    }

    #[must_use]
    pub fn with_source<ErrorType>(diagnostic: ExecutorDiagnostic, source: ErrorType) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        Self {
            diagnostic: Box::new(diagnostic),
            source: Some(Box::new(source)),
        }
    }

    #[must_use]
    pub fn with_cause(mut self, mut cause: ExecutorDiagnostic) -> Self {
        cause.cause = self.diagnostic.cause.take();
        self.diagnostic.cause = Some(Box::new(cause));
        self
    }

    #[must_use]
    pub fn diagnostic(&self) -> &ExecutorDiagnostic {
        &self.diagnostic
    }

    #[must_use]
    fn provider_subject(agent_name: String) -> ExecutorDiagnosticSubject {
        ExecutorDiagnosticSubject::Provider {
            agent_name,
            provider_name: None,
            model_name: None,
            attempt: None,
            http_status: None,
        }
    }
}

impl From<McpError> for ModelProviderError {
    fn from(error: McpError) -> Self {
        let diagnostic = ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::McpFailed,
            ExecutorStage::Mcp,
            "MCP operation failed",
            ExecutorDiagnosticSubject::Mcp {
                agent_name: None,
                server_name: None,
                target_name: None,
            },
        )
        .with_retryability(DiagnosticRetryability::Unknown);

        Self::with_source(diagnostic, error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_preserves_root_source() {
        let error = ModelProviderError::model_with_source(
            "writer",
            "provider request failed",
            std::io::Error::new(std::io::ErrorKind::TimedOut, "upstream timeout"),
        );
        let source = std::error::Error::source(&error).expect("provider error should expose its root source");

        assert_eq!(source.to_string(), "upstream timeout");
    }

    #[test]
    fn provider_source_text_is_not_part_of_public_diagnostic() {
        let sensitive_source_text = "reflected prompt secret-source-token";
        let error =
            ModelProviderError::model_with_source("writer", "provider request failed", std::io::Error::other(sensitive_source_text));
        let serialized_diagnostic = serde_json::to_string(error.diagnostic()).expect("provider diagnostic should serialize");

        assert!(!serialized_diagnostic.contains(sensitive_source_text));
        assert_eq!(
            std::error::Error::source(&error)
                .expect("provider error should retain its source privately")
                .to_string(),
            sensitive_source_text
        );
    }

    #[test]
    fn rejected_response_content_is_replaced_with_fixed_public_message() {
        let sensitive_response = "response content: reflected-secret-token";
        let error = ModelProviderError::rejected_with_source("writer", sensitive_response, std::io::Error::other("private parse failure"));
        let serialized_diagnostic = serde_json::to_string(error.diagnostic()).expect("rejection diagnostic should serialize");

        assert_eq!(error.diagnostic().message, "model response was rejected");
        assert!(!serialized_diagnostic.contains(sensitive_response));
        assert_eq!(
            std::error::Error::source(&error)
                .expect("rejection should retain its source privately")
                .to_string(),
            "private parse failure"
        );
    }
}
