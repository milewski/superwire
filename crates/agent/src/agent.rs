use crate::context::Context;
use crate::error::AgentError;
use crate::error::ExecutorError;
use crate::message::Message;
use crate::tool::{registered_runtime_tools, DynamicTool, RuntimeTool};
use crate::traits::{Executable, Provider};
use std::sync::Arc;

/// Configuration for the agent
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of tokens to generate for the model response.
    ///
    /// Mapped to provider-specific limits (for example `max_tokens`,
    /// `max_output_tokens`, or Ollama `num_predict`).
    pub max_tokens: Option<usize>,

    /// Sampling temperature.
    ///
    /// Higher values increase randomness; lower values make outputs more deterministic.
    pub temperature: Option<f32>,

    /// Nucleus sampling parameter.
    ///
    /// The model samples from the smallest token set whose cumulative probability
    /// reaches `top_p`.
    pub top_p: Option<f32>,

    /// Limits sampling to the `top_k` most likely next tokens.
    pub top_k: Option<u32>,

    /// Penalizes tokens based on how frequently they already appeared.
    ///
    /// Higher values reduce repetition frequency.
    pub frequency_penalty: Option<f32>,

    /// Penalizes tokens that already appeared at least once.
    ///
    /// Higher values encourage introducing new topics or terms.
    pub presence_penalty: Option<f32>,

    /// Penalizes repeating recent token sequences.
    ///
    /// Primarily used by Ollama-style generation options.
    pub repeat_penalty: Option<f32>,

    /// Random seed for reproducible sampling when supported by the provider.
    pub seed: Option<i32>,

    /// Stop sequences that terminate generation when matched.
    pub stop_sequences: Option<Vec<String>>,

    /// Number of recent messages used for repeated-loop detection.
    ///
    /// Lower values make loop detection more aggressive.
    /// Higher values allow more iterative back-and-forth.
    pub stuck_threshold: usize,

    /// Maximum number of automatic retries for retriable provider failures.
    ///
    /// Applies to transient issues like rate limits and temporary network problems.
    pub provider_max_retries: usize,

    /// Base delay (milliseconds) for exponential retry backoff.
    ///
    /// Effective delay grows per retry attempt (for example 500ms, 1000ms, 2000ms).
    pub provider_retry_base_delay_ms: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            frequency_penalty: None,
            presence_penalty: None,
            repeat_penalty: None,
            seed: None,
            stop_sequences: None,
            stuck_threshold: 5,
            provider_max_retries: 3,
            provider_retry_base_delay_ms: 500,
        }
    }
}

impl AgentConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    /// Limits how long the model response is allowed to be.
    ///
    /// Use this when you want shorter answers, lower cost, or to avoid very long outputs.
    /// Increase it if responses are getting cut off too early.
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    /// Controls randomness in the model output.
    ///
    /// - Lower values (for example `0.0` to `0.3`): more predictable, stable answers.
    /// - Medium values (for example `0.5` to `0.8`): balanced creativity.
    /// - Higher values (for example `1.0+`): more varied but potentially less reliable.
    ///
    /// Use low values for structured tasks and high values for brainstorming.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    #[must_use]
    /// Enables nucleus sampling (`top_p`) to limit token choices by probability mass.
    ///
    /// The model only samples from the smallest set of tokens whose combined
    /// probability reaches `top_p`.
    ///
    /// - Lower values: safer, more focused outputs.
    /// - Higher values: broader, more creative outputs.
    ///
    /// Use this as an alternative to high temperature when you want controlled variety.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    #[must_use]
    /// Limits sampling to the `top_k` most likely next tokens.
    ///
    /// Smaller values constrain the model to safer choices.
    /// Larger values allow more diversity.
    ///
    /// Mostly useful with providers that support top-k directly (for example Ollama).
    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    #[must_use]
    /// Reduces repeated wording by penalizing tokens that appear frequently.
    ///
    /// Use this when the model keeps repeating phrases or sentence patterns.
    /// Increase gradually to avoid making text unnatural.
    pub fn with_frequency_penalty(mut self, frequency_penalty: f32) -> Self {
        self.frequency_penalty = Some(frequency_penalty);
        self
    }

    #[must_use]
    /// Encourages the model to introduce new words and topics.
    ///
    /// Use this when answers feel too narrow or keep circling the same ideas.
    /// This is often helpful in ideation and exploratory writing.
    pub fn with_presence_penalty(mut self, presence_penalty: f32) -> Self {
        self.presence_penalty = Some(presence_penalty);
        self
    }

    #[must_use]
    /// Penalizes immediate local repetition (mostly for Ollama-style backends).
    ///
    /// Use this when generated text loops or repeats recent fragments.
    /// Keep values moderate to avoid hurting fluency.
    pub fn with_repeat_penalty(mut self, repeat_penalty: f32) -> Self {
        self.repeat_penalty = Some(repeat_penalty);
        self
    }

    #[must_use]
    /// Sets a random seed for reproducible outputs when the provider supports it.
    ///
    /// Use this for debugging, tests, and experiments where you want repeatable runs.
    pub fn with_seed(mut self, seed: i32) -> Self {
        self.seed = Some(seed);
        self
    }

    #[must_use]
    /// Stops generation when any of the given text sequences appears.
    ///
    /// Useful for structured protocols or custom delimiters (for example `"\nEND"`).
    /// Pick unique stop strings to avoid cutting off normal content by accident.
    pub fn with_stop_sequences(mut self, stop_sequences: Vec<String>) -> Self {
        self.stop_sequences = Some(stop_sequences);
        self
    }

    #[must_use]
    /// Sets how many recent messages are checked for repetition loops.
    ///
    /// Use a lower value to fail fast when the model repeats itself.
    /// Use a higher value if your workflow needs longer iterative cycles.
    pub fn with_stuck_threshold(mut self, stuck_threshold: usize) -> Self {
        self.stuck_threshold = stuck_threshold;
        self
    }

    #[must_use]
    /// Sets maximum retries for transient provider failures.
    ///
    /// Increase when operating under frequent rate limits or flaky networks.
    pub fn with_provider_max_retries(mut self, provider_max_retries: usize) -> Self {
        self.provider_max_retries = provider_max_retries;
        self
    }

    #[must_use]
    /// Sets base retry delay in milliseconds for provider backoff.
    ///
    /// Higher values reduce pressure on provider APIs when rate limited.
    pub fn with_provider_retry_base_delay_ms(mut self, provider_retry_base_delay_ms: u64) -> Self {
        self.provider_retry_base_delay_ms = provider_retry_base_delay_ms;
        self
    }
}

/// Aggregated execution statistics derived from context
#[derive(Debug, Clone)]
pub struct AgentRunStatistics {
    pub total_messages: usize,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub assistant_tool_call_messages: usize,
    pub tool_result_messages: usize,
    pub successful_tool_results: usize,
    pub failed_tool_results: usize,
    pub system_messages: usize,
    pub total_tokens: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

impl AgentRunStatistics {
    fn from_context(context: &Context) -> Self {
        let mut user_messages = 0;
        let mut assistant_messages = 0;
        let mut assistant_tool_call_messages = 0;
        let mut tool_result_messages = 0;
        let mut successful_tool_results = 0;
        let mut failed_tool_results = 0;
        let mut system_messages = 0;

        for message in &context.messages {
            match message {
                Message::User { content: _ } => {
                    user_messages += 1;
                }

                Message::Assistant { content: _ } => {
                    assistant_messages += 1;
                }

                Message::AssistantToolCall { tool: _ } => {
                    assistant_tool_call_messages += 1;
                }

                Message::ToolResult { result } => {
                    tool_result_messages += 1;

                    match result {
                        crate::message::ToolResult::Success {
                            tool_call_id: _,
                            content: _,
                        } => {
                            successful_tool_results += 1;
                        }

                        crate::message::ToolResult::Failure {
                            tool_call_id: _,
                            content: _,
                        } => {
                            failed_tool_results += 1;
                        }
                    }
                }

                Message::System { content: _ } => {
                    system_messages += 1;
                }
            }
        }

        Self {
            total_messages: context.messages.len(),
            user_messages,
            assistant_messages,
            assistant_tool_call_messages,
            tool_result_messages,
            successful_tool_results,
            failed_tool_results,
            system_messages,
            total_tokens: context.total_tokens,
            input_tokens: context.input_tokens,
            output_tokens: context.output_tokens,
        }
    }
}

/// Result payload returned from agent execution
#[derive(Debug, Clone)]
pub struct AgentRunResult<TOutput> {
    pub output: TOutput,
    pub context: Context,
    pub statistics: AgentRunStatistics,
}

/// The main agent that executes once without retry logic
pub struct Agent<E, P>
where
    E: Executable<Provider = P>,
    P: Provider,
{
    executor: E,
    provider: P,
    tools: Vec<Arc<dyn RuntimeTool>>,
    config: AgentConfig,
}

impl<E, P> Agent<E, P>
where
    E: Executable<Provider = P>,
    E::Error: Into<ExecutorError>,
    P: Provider,
{
    pub fn new(executor: E, provider: P) -> Self {
        Self::new_without_registered_tools(executor, provider).with_registered_tools()
    }

    pub fn new_without_registered_tools(executor: E, provider: P) -> Self {
        Self {
            executor,
            provider,
            tools: Vec::new(),
            config: AgentConfig::default(),
        }
    }

    #[must_use]
    pub fn with_registered_tools(mut self) -> Self {
        self.tools.extend(registered_runtime_tools());
        self
    }

    #[must_use]
    pub fn with_runtime_tool(mut self, runtime_tool: Arc<dyn RuntimeTool>) -> Self {
        self.tools.push(runtime_tool);
        self
    }

    #[must_use]
    pub fn with_dynamic_tool(mut self, dynamic_tool: DynamicTool) -> Self {
        self.tools.push(Arc::new(dynamic_tool));
        self
    }

    #[must_use]
    pub fn with_tool<T: RuntimeTool + Default + 'static>(mut self) -> Self {
        self.tools.push(Arc::new(T::default()));
        self
    }

    #[must_use]
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn run(&self, prompt: impl Into<String>) -> Result<AgentRunResult<E::Output>, AgentError> {
        self.run_with_context(Context::new(), prompt).await
    }

    pub async fn run_with_context(&self, mut context: Context, prompt: impl Into<String>) -> Result<AgentRunResult<E::Output>, AgentError> {
        context.add_user_message(prompt);

        let output = match self.executor.execute(&mut context, &self.provider, &self.tools, &self.config).await {
            Ok(result) => result,
            Err(error) => {
                return Err(AgentError::ExecutionFailed {
                    error: error.into(),
                    context,
                });
            }
        };

        let statistics = AgentRunStatistics::from_context(&context);

        Ok(AgentRunResult {
            output,
            context,
            statistics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Agent;
    use crate::tests::executor_support::MockProvider;
    use crate::tool::{DynamicTool, FinalizeTool, Tool, ToolError};
    use crate::{assert_has_tool_success_content, assert_tool_result, provider, tool_call, LoopExecutor};
    use async_trait::async_trait;
    use schemars::schema_for;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::json;
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    struct Person {
        name: String,
        age: usize,
    }

    #[derive(Debug, Clone, Default)]
    struct InventoryEchoTool;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct InventoryEchoInput {
        value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct DynamicEchoInput {
        value: String,
    }

    #[async_trait]
    impl Tool for InventoryEchoTool {
        type Input = InventoryEchoInput;

        fn name(&self) -> &str {
            "inventory_echo_tool_for_agent_test"
        }

        fn description(&self) -> &str {
            "Echo tool registered with inventory"
        }

        async fn execute(&self, input: Self::Input) -> Result<Value, ToolError> {
            Ok(json!({ "echo": input.value }))
        }
    }

    crate::register_tool!(InventoryEchoTool);

    #[tokio::test]
    async fn automatically_executes_inventory_registered_tools() {
        let provider = provider!([
            tool_call!(InventoryEchoTool, id = "inventory-echo", { "value": "hello" }),
            tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })
        ]);

        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");

        let result = Agent::new(executor, provider)
            .run("Use the inventory registered tool")
            .await
            .expect("agent should execute inventory tool");

        assert_eq!(
            result.output,
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_tool_result!(result.context, "inventory-echo");
        assert_has_tool_success_content!(result.context, { "echo": "hello" });
    }

    #[tokio::test]
    async fn executes_dynamic_tools_defined_at_runtime() {
        let dynamic_tool = DynamicTool::from_parts(
            "ffi_echo",
            "Echoes runtime JSON input",
            schema_for!(DynamicEchoInput),
            |input| async move {
                let echoed_value = input
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::new("Expected a string field named 'value'"))?;

                Ok(json!({ "echo": echoed_value }))
            },
        );

        let provider = provider!([
            crate::ToolCall {
                id: "dynamic-echo".to_string(),
                name: "ffi_echo".to_string(),
                arguments: json!({ "value": "from ffi" }),
            },
            tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })
        ]);

        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");

        let result = Agent::new_without_registered_tools(executor, provider)
            .with_dynamic_tool(dynamic_tool)
            .run("Use the dynamic tool")
            .await
            .expect("agent should execute runtime dynamic tool");

        assert_eq!(
            result.output,
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_tool_result!(result.context, "dynamic-echo");
        assert_has_tool_success_content!(result.context, { "echo": "from ffi" });
    }
}
