use super::context::Context;
use super::message::ToolCall;

/// Trait for tools that can be used by the agent
pub trait Tool: Clone {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> schemars::Schema;
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
    type Input;
    type Tool: Tool;

    async fn generate(&self, context: &Context<Self::Input, Self::Tool>) -> Result<ProviderResponse, String>;
}

/// Trait for operations that can be executed by the agent
#[async_trait::async_trait]
pub trait Executable {
    type Input;
    type Output;
    type Provider: Provider<Tool = Self::Tool>;
    type Tool: Tool;

    async fn execute(
        &self,
        context: &Context<Self::Input, Self::Tool>,
        provider: &Self::Provider,
    ) -> Result<Self::Output, String>;
}

/// Trait for validating output against a schema
#[async_trait::async_trait]
pub trait Validator {
    type Output;

    async fn validate(&self, output: &Self::Output) -> Result<(), super::error::ValidationError>;
}
