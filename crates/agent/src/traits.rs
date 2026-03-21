use crate::context::Context;
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

/// Execution result containing final output and context
#[derive(Debug, Clone)]
pub struct ExecutionResult<Output> {
    pub output: Output,
    pub context: Context,
}

/// Trait for LLM providers that can generate responses
#[async_trait::async_trait]
pub trait Provider {
    async fn generate(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<ProviderResponse, String>;
}

/// Trait for operations that can be executed by the agent
#[async_trait::async_trait]
pub trait Executable {
    type Output;
    type Error;
    type Provider: Provider;

    async fn execute(
        &self,
        context: &Context,
        provider: &Self::Provider,
        tools: &[Arc<dyn RuntimeTool>],
        config: &AgentConfig,
    ) -> Result<ExecutionResult<Self::Output>, Self::Error>;
}
