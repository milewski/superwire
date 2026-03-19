use crate::context::Context;
use crate::error::ValidationError;
use crate::message::ToolCall;
use crate::tool::RuntimeTool;
use schemars::Schema;
use std::sync::Arc;

/// Provider-facing tool definition
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: Schema,
}

/// Reason why the provider stopped generating
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// End of sequence token was reached
    EndOfSequence,
    /// Maximum tokens limit was reached
    MaxTokens,
    /// Tool calls were generated
    ToolCalls,
    /// Content filter was triggered
    ContentFilter,
    /// Stop sequence was encountered
    StopSequence,
    /// Other/unknown reason
    Other(String),
}

/// Provider response containing tool calls and optional text
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub tool_calls: Vec<ToolCall>,
    pub text: Option<String>,
    pub stop_reason: StopReason,
}

/// Trait for LLM providers that can generate responses
#[async_trait::async_trait]
pub trait Provider {
    async fn generate(&self, context: &Context, tools: &[ToolDefinition]) -> Result<ProviderResponse, String>;
}

/// Trait for operations that can be executed by the agent
#[async_trait::async_trait]
pub trait Executable {
    type Prompt;
    type Output;
    type Provider: Provider;

    async fn execute(
        &self,
        context: &Context,
        provider: &Self::Provider,
        tools: &[Arc<dyn RuntimeTool>],
    ) -> Result<Self::Output, String>;
}

/// Trait for validating output against a schema
#[async_trait::async_trait]
pub trait Validator {
    type Output;

    async fn validate(&self, output: &Self::Output) -> Result<(), ValidationError>;
}
