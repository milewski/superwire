use serde_json::Value;
use superwire_agent::AgentError;
use superwire_core::runtime::WorkflowRuntimeError;

use crate::types::{ToolInvocationError, ToolInvocationErrorCode, WorkflowExecutionError, WorkflowExecutionErrorCode};

impl WorkflowExecutionError {
    #[must_use]
    pub fn parse_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::ParseFailed,
            message,
            context: None,
            details,
        }
    }

    #[must_use]
    pub fn validation_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::ValidationFailed,
            message,
            context: None,
            details,
        }
    }

    #[must_use]
    pub fn runtime_failed(message: String, context: Option<Value>, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::RuntimeFailed,
            message,
            context,
            details,
        }
    }

    #[must_use]
    pub fn tool_invocation_failed(message: String, details: Option<Value>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::ToolInvocationFailed,
            message,
            context: None,
            details,
        }
    }

    #[must_use]
    pub fn internal(message: String) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::Internal,
            message,
            context: None,
            details: None,
        }
    }

    #[must_use]
    pub fn from_runtime_error(runtime_error: WorkflowRuntimeError) -> Self {
        match runtime_error {
            WorkflowRuntimeError::ParseFailed { source: _, details } => Self::parse_failed(details, None),
            WorkflowRuntimeError::InvalidWorkflow { issues }
            | WorkflowRuntimeError::ExecutionPlanInvariant { message: issues }
            | WorkflowRuntimeError::MissingDeclaration { message: issues }
            | WorkflowRuntimeError::UnsupportedFeature { feature: issues }
            | WorkflowRuntimeError::InputTypeMismatch {
                expected: issues,
                found: _,
            }
            | WorkflowRuntimeError::OutputTypeMismatch {
                expected: issues,
                found: _,
            } => Self::validation_failed(issues, None),
            WorkflowRuntimeError::ProviderConfiguration { provider_name, message } => {
                Self::runtime_failed(format!("provider `{provider_name}` configuration error: {message}"), None, None)
            }
            WorkflowRuntimeError::ExpressionEvaluation { context, message } => {
                Self::runtime_failed(format!("expression evaluation failed in {context}: {message}"), None, None)
            }
            WorkflowRuntimeError::InvalidAgentProperty {
                agent_name,
                property,
                message,
            } => Self::runtime_failed(
                format!("agent `{agent_name}` has invalid `{property}` property: {message}"),
                None,
                None,
            ),
            WorkflowRuntimeError::InputValueMismatch { message } => Self::runtime_failed(message, None, None),
            WorkflowRuntimeError::AgentOutputTypeMismatch { agent_name, message } => Self::runtime_failed(
                format!("agent `{agent_name}` output does not match declared type: {message}"),
                None,
                None,
            ),
            WorkflowRuntimeError::AgentExecutionFailed { agent_name, source } => {
                if let AgentError::ExecutionFailed { error, context } = source.as_ref() {
                    let agent_context = serde_json::to_value(context).ok();

                    Self::runtime_failed(format!("agent `{agent_name}` execution failed: {error}"), agent_context, None)
                } else {
                    Self::runtime_failed(format!("agent `{agent_name}` execution failed: {source}"), None, None)
                }
            }
            WorkflowRuntimeError::SerializationFailed { context, source } => {
                Self::runtime_failed(format!("serialization failed for {context}: {source}"), None, None)
            }
            WorkflowRuntimeError::OutputDeserializationFailed { source } => {
                Self::runtime_failed(format!("output deserialization failed: {source}"), None, None)
            }
            WorkflowRuntimeError::Other { message } => Self::runtime_failed(message, None, None),
        }
    }
}

impl ToolInvocationError {
    #[must_use]
    pub fn tool_not_found(message: String) -> Self {
        Self {
            code: ToolInvocationErrorCode::ToolNotFound,
            message,
            details: None,
        }
    }

    #[must_use]
    pub fn internal(message: String) -> Self {
        Self {
            code: ToolInvocationErrorCode::Internal,
            message,
            details: None,
        }
    }
}
