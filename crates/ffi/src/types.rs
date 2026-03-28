use serde::{Deserialize, Serialize};

use crate::error::FfiError;

pub const FFI_PROTOCOL_VERSION: u32 = 1;

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FfiRequestEnvelope {
    pub protocol_version: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    #[serde(flatten)]
    pub request: FfiRequest,
}

impl FfiRequestEnvelope {
    #[must_use]
    pub fn new(request: FfiRequest) -> Self {
        Self {
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id: None,
            request,
        }
    }

    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());

        self
    }

    #[must_use]
    pub fn operation(&self) -> FfiOperation {
        self.request.operation()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FfiResponseEnvelope {
    pub protocol_version: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    #[serde(flatten)]
    pub response: FfiResponse,
}

impl FfiResponseEnvelope {
    #[must_use]
    pub fn new(response: FfiResponse) -> Self {
        Self {
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id: None,
            response,
        }
    }

    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());

        self
    }

    #[must_use]
    pub fn operation(&self) -> FfiOperation {
        self.response.operation()
    }

    #[must_use]
    pub fn from_operation_error(operation: FfiOperation, request_id: Option<String>, error: &FfiError) -> Self {
        let response = match operation {
            FfiOperation::ExecuteWorkflow => FfiResponse::ExecuteWorkflow(WorkflowExecutionEnvelope::Failed {
                error: WorkflowExecutionError {
                    code: WorkflowExecutionErrorCode::Internal,
                    message: error.to_string(),
                    details: None,
                },
            }),
            FfiOperation::InvokeTool => FfiResponse::InvokeTool(ToolInvocationEnvelope::Failed {
                error: ToolInvocationError {
                    code: ToolInvocationErrorCode::Internal,
                    message: error.to_string(),
                    details: None,
                },
            }),
            FfiOperation::ReadExecutionValue => FfiResponse::ReadExecutionValue(ReadExecutionValueEnvelope::Failed {
                error: WorkflowExecutionError {
                    code: WorkflowExecutionErrorCode::Internal,
                    message: error.to_string(),
                    details: None,
                },
            }),
        };

        Self {
            protocol_version: FFI_PROTOCOL_VERSION,
            request_id,
            response,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FfiOperation {
    ExecuteWorkflow,
    InvokeTool,
    ReadExecutionValue,
}

impl FfiOperation {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecuteWorkflow => "execute_workflow",
            Self::InvokeTool => "invoke_tool",
            Self::ReadExecutionValue => "read_execution_value",
        }
    }
}

impl std::fmt::Display for FfiOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", content = "payload", rename_all = "snake_case")]
pub enum FfiRequest {
    ExecuteWorkflow(WorkflowExecutionRequest),
    InvokeTool(ToolInvocationPayload),
    ReadExecutionValue(ReadExecutionValueRequest),
}

impl FfiRequest {
    #[must_use]
    pub fn operation(&self) -> FfiOperation {
        match self {
            Self::ExecuteWorkflow(_) => FfiOperation::ExecuteWorkflow,
            Self::InvokeTool(_) => FfiOperation::InvokeTool,
            Self::ReadExecutionValue(_) => FfiOperation::ReadExecutionValue,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", content = "payload", rename_all = "snake_case")]
pub enum FfiResponse {
    ExecuteWorkflow(WorkflowExecutionEnvelope),
    InvokeTool(ToolInvocationEnvelope),
    ReadExecutionValue(ReadExecutionValueEnvelope),
}

impl FfiResponse {
    #[must_use]
    pub fn operation(&self) -> FfiOperation {
        match self {
            Self::ExecuteWorkflow(_) => FfiOperation::ExecuteWorkflow,
            Self::InvokeTool(_) => FfiOperation::InvokeTool,
            Self::ReadExecutionValue(_) => FfiOperation::ReadExecutionValue,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowExecutionRequest {
    pub execution_id: String,
    pub workflow_source: String,
    pub input: WorkflowExecutionInput,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<WorkflowExecutionSecrets>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_tools: Vec<CustomToolDeclaration>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_callback: Option<ToolCallbackConfig>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub defer_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallbackConfig {
    pub endpoint: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowExecutionInput {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowExecutionSecrets {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomToolDeclaration {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    pub input_schema: serde_json::Value,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInvocationPayload {
    pub execution_id: String,
    pub invocation_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInvocationResult {
    pub execution_id: String,
    pub invocation_id: String,
    pub output: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInvocationError {
    pub code: ToolInvocationErrorCode,
    pub message: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationErrorCode {
    ToolNotFound,
    InvalidArguments,
    ExecutionFailed,
    Timeout,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolInvocationEnvelope {
    Succeeded { result: ToolInvocationResult },
    Failed { error: ToolInvocationError },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowExecutionOutput {
    pub execution_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadExecutionValueRequest {
    pub execution_id: String,
    pub value: ExecutionValueName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionValueName {
    Success,
    Error,
    Context,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadExecutionValueSuccess {
    pub execution_id: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReadExecutionValueEnvelope {
    Succeeded { result: ReadExecutionValueSuccess },
    Failed { error: WorkflowExecutionError },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowExecutionError {
    pub code: WorkflowExecutionErrorCode,
    pub message: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionErrorCode {
    ParseFailed,
    ValidationFailed,
    RuntimeFailed,
    ToolInvocationFailed,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkflowExecutionEnvelope {
    Succeeded { output: WorkflowExecutionOutput },
    Failed { error: WorkflowExecutionError },
}
