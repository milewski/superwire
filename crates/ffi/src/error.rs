use engine_ai_core::runtime::WorkflowRuntimeError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FfiErrorCode {
    InvalidRequest,
    WorkflowParseFailed,
    WorkflowValidationFailed,
    WorkflowExecutionFailed,
    SerializationFailed,
    ToolInvocationFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiError {
    pub code: FfiErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl FfiError {
    #[must_use]
    pub fn new(code: FfiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    #[must_use]
    pub fn from_workflow_runtime_error(error: WorkflowRuntimeError) -> Self {
        let error_code = Self::workflow_runtime_error_code(&error);

        Self {
            code: error_code,
            message: error.to_string(),
            details: None,
        }
    }

    fn workflow_runtime_error_code(error: &WorkflowRuntimeError) -> FfiErrorCode {
        match error {
            WorkflowRuntimeError::ParseFailed { source: _, details: _ } => FfiErrorCode::WorkflowParseFailed,
            WorkflowRuntimeError::InvalidWorkflow { issues: _ }
            | WorkflowRuntimeError::ExecutionPlanInvariant { message: _ }
            | WorkflowRuntimeError::MissingDeclaration { message: _ }
            | WorkflowRuntimeError::UnsupportedFeature { feature: _ }
            | WorkflowRuntimeError::ProviderConfiguration {
                provider_name: _,
                message: _,
            }
            | WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: _,
                property: _,
                message: _,
            }
            | WorkflowRuntimeError::InputTypeMismatch { expected: _, found: _ }
            | WorkflowRuntimeError::OutputTypeMismatch { expected: _, found: _ }
            | WorkflowRuntimeError::InputValueMismatch { message: _ }
            | WorkflowRuntimeError::AgentOutputTypeMismatch { agent_name: _, message: _ } => FfiErrorCode::WorkflowValidationFailed,
            WorkflowRuntimeError::SerializationFailed { context: _, source: _ }
            | WorkflowRuntimeError::OutputDeserializationFailed { source: _ } => FfiErrorCode::SerializationFailed,
            WorkflowRuntimeError::ExpressionEvaluation { context: _, message: _ }
            | WorkflowRuntimeError::AgentExecutionFailed { agent_name: _, source: _ }
            | WorkflowRuntimeError::Other { message: _ } => FfiErrorCode::WorkflowExecutionFailed,
        }
    }
}

impl Display for FfiError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for FfiError {}

impl From<WorkflowRuntimeError> for FfiError {
    fn from(error: WorkflowRuntimeError) -> Self {
        Self::from_workflow_runtime_error(error)
    }
}
