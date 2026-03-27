use crate::error::FfiError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: Value,
}

impl ForeignToolDefinition {
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters_json_schema: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_json_schema,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

impl ToolInvocationRequest {
    #[must_use]
    pub fn new(tool_call_id: impl Into<String>, tool_name: impl Into<String>, arguments: Value) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            tool_name: tool_name.into(),
            arguments,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationErrorCode {
    ToolNotFound,
    InvalidArguments,
    ExecutionFailed,
    Timeout,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationError {
    pub code: ToolInvocationErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl ToolInvocationError {
    #[must_use]
    pub fn new(code: ToolInvocationErrorCode, message: impl Into<String>) -> Self {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolInvocationResult {
    Success { content: Value },
    Failure { error: ToolInvocationError },
}

impl ToolInvocationResult {
    #[must_use]
    pub fn success(content: Value) -> Self {
        Self::Success { content }
    }

    #[must_use]
    pub fn failure(error: ToolInvocationError) -> Self {
        Self::Failure { error }
    }
}

#[async_trait]
pub trait ForeignToolRuntime: Send + Sync {
    fn tool_definitions(&self) -> Result<Vec<ForeignToolDefinition>, FfiError>;

    async fn invoke_tool(&self, request: ToolInvocationRequest) -> Result<ToolInvocationResult, FfiError>;
}
