use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::error::ProviderError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderRequest {
    pub prompt: String,
    pub tools: Vec<ToolDefinition>,
    pub response_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderResponse {
    pub message: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderModelConfig {
    pub provider_name: String,
    pub model_name: String,
    pub api_endpoint: Option<String>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(
        &self,
        model: &ProviderModelConfig,
        request: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError>;

    fn driver(&self) -> &'static str;
}
