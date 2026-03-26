use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionRequest {
    pub workflow_file_path: String,
    pub workflow_input: Value,
    #[serde(default)]
    pub custom_tools: CustomToolRegistry,
}

impl WorkflowExecutionRequest {
    pub fn from_json(request_json: &str) -> Result<Self, FfiError> {
        serde_json::from_str(request_json).map_err(FfiError::InvalidRequest)
    }

    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CustomToolRegistry {
    #[serde(default)]
    pub definitions: Vec<CustomToolDefinition>,
}

impl CustomToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, definition: CustomToolDefinition) {
        self.definitions.push(definition);
    }

    pub fn registered_definitions(&self) -> &[CustomToolDefinition] {
        self.definitions.as_slice()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub execution_contract: CustomToolExecutionContract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CustomToolExecutionContract {
    #[default]
    HostCallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInvocationRequest {
    pub tool_name: String,
    pub tool_input: Value,
}

impl ToolInvocationRequest {
    pub fn from_json(request_json: &str) -> Result<Self, FfiError> {
        serde_json::from_str(request_json).map_err(FfiError::InvalidRequest)
    }

    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInvocationResponse {
    pub result: ToolInvocationResult,
}

impl ToolInvocationResponse {
    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolInvocationResult {
    Succeeded { tool_output: Value },
    Failed { error: WorkflowExecutionError },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionResponse {
    pub result: WorkflowExecutionResult,
}

impl WorkflowExecutionResponse {
    pub fn to_json(&self) -> Result<String, FfiError> {
        serde_json::to_string(self).map_err(FfiError::Serialization)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkflowExecutionResult {
    Succeeded { workflow_output: Value },
    Failed { error: WorkflowExecutionError },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionError {
    pub code: WorkflowExecutionErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl WorkflowExecutionError {
    pub fn runtime_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: WorkflowExecutionErrorCode::RuntimeUnavailable,
            message: message.into(),
            details: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionErrorCode {
    InvalidRequest,
    WorkflowLoadFailed,
    ToolRegistrationFailed,
    ToolExecutionFailed,
    WorkflowExecutionFailed,
    RuntimeUnavailable,
}

#[derive(Debug, Error)]
pub enum FfiError {
    #[error("failed to deserialize ffi request: {0}")]
    InvalidRequest(serde_json::Error),
    #[error("failed to serialize ffi response: {0}")]
    Serialization(serde_json::Error),
    #[error("ffi workflow execution runtime is not implemented yet")]
    RuntimeNotImplemented,
}

pub trait WorkflowExecutor {
    fn execute_workflow(&self, request: WorkflowExecutionRequest) -> Result<WorkflowExecutionResponse, FfiError>;
}

#[derive(Debug, Default)]
pub struct FfiInterface;

impl FfiInterface {
    pub fn execute_workflow_from_json(&self, request_json: &str) -> Result<String, FfiError> {
        let request = WorkflowExecutionRequest::from_json(request_json)?;
        let response = self.execute_workflow(request)?;

        response.to_json()
    }

    pub fn execute_workflow(&self, _request: WorkflowExecutionRequest) -> Result<WorkflowExecutionResponse, FfiError> {
        Err(FfiError::RuntimeNotImplemented)
    }
}

#[cfg(feature = "php-ext")]
pub fn php_extension_enabled() -> bool {
    true
}
