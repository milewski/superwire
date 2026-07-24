use std::error::Error;
use superwire_dsl::ModelAssetKindSupportError;
use superwire_mcp::McpError;
use superwire_model::ModelProviderError;
use superwire_protocol::event::{
    CacheOperation, DiagnosticRetryability, DiagnosticSeverity, ExecutorDiagnostic, ExecutorDiagnosticCode, ExecutorDiagnosticSubject,
    ExecutorEvent, ExecutorStage,
};
use superwire_semantic::WorkflowSemanticError;
use thiserror::Error;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error(transparent)]
    Semantic(#[from] WorkflowSemanticError),

    #[error(transparent)]
    Mcp(#[from] McpError),

    #[error("workflow input type mismatch: expected `{expected}`, found `{found}`")]
    InputTypeMismatch { expected: String, found: String },

    #[error("workflow input value does not match declared `input` block type: {message}")]
    InputValueMismatch { message: String },

    #[error("workflow secrets value does not match declared `secrets` block type: {message}")]
    SecretValueMismatch { message: String },

    #[error("workflow output type mismatch: expected `{expected}`, found `{found}`")]
    OutputTypeMismatch { expected: String, found: String },

    #[error("agent `{agent_name}` output does not match declared output type: {message}")]
    AgentOutputTypeMismatch { agent_name: String, message: String },

    #[error(transparent)]
    Model(#[from] ModelProviderError),

    #[error("{diagnostic}")]
    Diagnostic {
        diagnostic: Box<ExecutorDiagnostic>,

        #[source]
        source: Option<BoxError>,
    },

    #[error("{message}")]
    Other { message: String },
}

impl From<ModelAssetKindSupportError> for ExecutorError {
    fn from(error: ModelAssetKindSupportError) -> Self {
        Self::Semantic(WorkflowSemanticError::from(error))
    }
}

impl ExecutorError {
    #[must_use]
    pub fn diagnostic(&self) -> ExecutorDiagnostic {
        match self {
            Self::Semantic(error) => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidWorkflow,
                ExecutorStage::Planning,
                error.to_string(),
                ExecutorDiagnosticSubject::Workflow,
            ),
            Self::Mcp(error) if error.is_network_policy_violation() => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidConfiguration,
                ExecutorStage::Mcp,
                error.public_message(),
                ExecutorDiagnosticSubject::Mcp {
                    agent_name: None,
                    server_name: None,
                    target_name: None,
                },
            ),
            Self::Mcp(error) => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::McpFailed,
                ExecutorStage::Mcp,
                error.public_message(),
                ExecutorDiagnosticSubject::Mcp {
                    agent_name: None,
                    server_name: None,
                    target_name: None,
                },
            )
            .with_retryability(DiagnosticRetryability::Unknown),
            Self::InputTypeMismatch { expected, found } => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidInput,
                ExecutorStage::Input,
                format!("workflow input type mismatch: expected `{expected}`, found `{found}`"),
                ExecutorDiagnosticSubject::Workflow,
            ),
            Self::InputValueMismatch { message } => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidInput,
                ExecutorStage::Input,
                format!("workflow input value does not match declared `input` block type: {message}"),
                ExecutorDiagnosticSubject::Workflow,
            ),
            Self::SecretValueMismatch { message } => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidSecrets,
                ExecutorStage::Secrets,
                format!("workflow secrets value does not match declared `secrets` block type: {message}"),
                ExecutorDiagnosticSubject::Workflow,
            ),
            Self::OutputTypeMismatch { expected, found } => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidOutput,
                ExecutorStage::Output,
                format!("workflow output type mismatch: expected `{expected}`, found `{found}`"),
                ExecutorDiagnosticSubject::Workflow,
            ),
            Self::AgentOutputTypeMismatch { agent_name, message } => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidOutput,
                ExecutorStage::Output,
                format!("agent `{agent_name}` output does not match declared output type: {message}"),
                ExecutorDiagnosticSubject::Agent {
                    agent_name: agent_name.clone(),
                    iteration_index: None,
                },
            ),
            Self::Model(error) => error.diagnostic().clone(),
            Self::Diagnostic { diagnostic, source: _ } => diagnostic.as_ref().clone(),
            Self::Other { message } => ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InternalError,
                ExecutorStage::Internal,
                message.clone(),
                ExecutorDiagnosticSubject::Workflow,
            ),
        }
    }

    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InvalidInput,
                ExecutorStage::Input,
                message,
                ExecutorDiagnosticSubject::Workflow,
            )),
            source: None,
        }
    }

    #[must_use]
    pub fn cache(operation: CacheOperation, message: impl Into<String>) -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(
                ExecutorDiagnostic::error(
                    ExecutorDiagnosticCode::CacheUnavailable,
                    ExecutorStage::Cache,
                    message,
                    ExecutorDiagnosticSubject::Cache { operation },
                )
                .with_retryability(DiagnosticRetryability::Safe),
            ),
            source: None,
        }
    }

    pub fn cache_with_source<ErrorType>(operation: CacheOperation, message: impl Into<String>, source: ErrorType) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        Self::Diagnostic {
            diagnostic: Box::new(
                ExecutorDiagnostic::error(
                    ExecutorDiagnosticCode::CacheUnavailable,
                    ExecutorStage::Cache,
                    message,
                    ExecutorDiagnosticSubject::Cache { operation },
                )
                .with_retryability(DiagnosticRetryability::Safe),
            ),
            source: Some(Box::new(source)),
        }
    }

    #[must_use]
    pub fn internal_panic(message: impl Into<String>) -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InternalPanic,
                ExecutorStage::Internal,
                message,
                ExecutorDiagnosticSubject::Workflow,
            )),
            source: None,
        }
    }

    #[must_use]
    pub fn internal_with_source<ErrorType>(message: impl Into<String>, source: ErrorType) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InternalError,
                ExecutorStage::Internal,
                message,
                ExecutorDiagnosticSubject::Workflow,
            )),
            source: Some(Box::new(source)),
        }
    }

    #[must_use]
    pub fn mcp_with_source<ErrorType>(
        agent_name: Option<String>,
        server_name: Option<String>,
        target_name: Option<String>,
        message: impl Into<String>,
        source: ErrorType,
    ) -> Self
    where
        ErrorType: Error + Send + Sync + 'static,
    {
        let diagnostic_code = if (&source as &dyn Error)
            .downcast_ref::<McpError>()
            .is_some_and(McpError::is_network_policy_violation)
        {
            ExecutorDiagnosticCode::InvalidConfiguration
        } else {
            ExecutorDiagnosticCode::McpFailed
        };

        Self::Diagnostic {
            diagnostic: Box::new(
                ExecutorDiagnostic::error(
                    diagnostic_code,
                    ExecutorStage::Mcp,
                    message,
                    ExecutorDiagnosticSubject::Mcp {
                        agent_name,
                        server_name,
                        target_name,
                    },
                )
                .with_retryability(DiagnosticRetryability::Unknown),
            ),
            source: Some(Box::new(source)),
        }
    }

    #[must_use]
    pub fn stream_gap(requested_after: Option<u64>, oldest_available: u64) -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::StreamGap,
                ExecutorStage::Stream,
                format!("requested event history is no longer retained; oldest available event is {oldest_available}"),
                ExecutorDiagnosticSubject::Stream {
                    requested_after,
                    oldest_available: Some(oldest_available),
                },
            )),
            source: None,
        }
    }

    #[must_use]
    pub fn stream_capacity_exceeded() -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::stream_capacity_exceeded()),
            source: None,
        }
    }

    #[must_use]
    pub fn stream_expired() -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::StreamExpired,
                ExecutorStage::Stream,
                "streamed execution history has expired",
                ExecutorDiagnosticSubject::Stream {
                    requested_after: None,
                    oldest_available: None,
                },
            )),
            source: None,
        }
    }

    #[must_use]
    pub fn unknown_run() -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::UnknownRun,
                ExecutorStage::Stream,
                "streamed execution was not found",
                ExecutorDiagnosticSubject::Stream {
                    requested_after: None,
                    oldest_available: None,
                },
            )),
            source: None,
        }
    }

    #[must_use]
    pub fn cancellation_conflict() -> Self {
        Self::Diagnostic {
            diagnostic: Box::new(ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::CancellationConflict,
                ExecutorStage::Cancellation,
                "streamed execution is already terminal",
                ExecutorDiagnosticSubject::Stream {
                    requested_after: None,
                    oldest_available: None,
                },
            )),
            source: None,
        }
    }

    #[must_use]
    pub fn cancellation_diagnostic() -> ExecutorDiagnostic {
        ExecutorDiagnostic::error(
            ExecutorDiagnosticCode::Cancelled,
            ExecutorStage::Cancellation,
            "workflow execution was cancelled",
            ExecutorDiagnosticSubject::Workflow,
        )
    }

    #[must_use]
    pub fn cache_degraded_event(&self, agent_name: Option<String>) -> ExecutorEvent {
        let mut diagnostic = self.diagnostic();

        diagnostic.severity = DiagnosticSeverity::Warning;

        ExecutorEvent::cache_degraded(agent_name, diagnostic)
    }

    #[must_use]
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::Semantic(WorkflowSemanticError::ParseFailed { .. } | WorkflowSemanticError::InvalidWorkflow { .. })
                | Self::InputTypeMismatch { .. }
                | Self::InputValueMismatch { .. }
                | Self::SecretValueMismatch { .. }
        ) || matches!(
            self.diagnostic().code,
            ExecutorDiagnosticCode::InvalidWorkflow
                | ExecutorDiagnosticCode::InvalidInput
                | ExecutorDiagnosticCode::InvalidSecrets
                | ExecutorDiagnosticCode::InvalidConfiguration
        )
    }
}
