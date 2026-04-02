use crate::context::Context;
use crate::error::ProviderError;
use crate::message::ToolCall;
use crate::tool::RuntimeTool;
use crate::AgentConfig;
use schemars::Schema;
use std::fmt::Debug;
use std::sync::Arc;

/// Provider-facing tool definition
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: Schema,
}

/// Tool-selection mode requested by the executor for a provider turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderToolChoice {
    /// Provider can decide whether to call tools or return plain text.
    Auto,

    /// Provider must return at least one tool call.
    Required,
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
    pub provider_message_id: Option<String>,
    pub stop_reason: StopReason,
    pub usage: Option<TokenUsage>,
}

/// Token usage metrics returned by providers.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub total_tokens: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

/// Trait for LLM providers that can generate responses
#[async_trait::async_trait]
pub trait Provider {
    async fn generate(
        &self,
        context: &Context,
        tools: &[ToolDefinition],
        tool_choice: ProviderToolChoice,
        config: &AgentConfig,
    ) -> Result<ProviderResponse, ProviderError>;
}

/// Trait for operations that can be executed by the agent
#[async_trait::async_trait]
pub trait Executable {
    type Output;
    type Error;
    type Provider: Provider;

    async fn execute(
        &self,
        context: &mut Context,
        provider: &Self::Provider,
        tools: &[Arc<dyn RuntimeTool>],
        config: &AgentConfig,
    ) -> Result<Self::Output, Self::Error>;
}
