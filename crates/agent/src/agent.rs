use crate::context::Context;
use crate::error::AgentError;
use crate::error::ExecutorError;
use crate::message::Message;
use crate::tool::RuntimeTool;
use crate::traits::{Executable, Provider};
use std::sync::Arc;

/// Configuration for the agent
#[derive(Default)]
pub struct AgentConfig {
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
}

impl AgentConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
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
        Self {
            executor,
            provider,
            tools: Vec::new(),
            config: AgentConfig {
                max_tokens: None,
                temperature: None,
            },
        }
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
        let mut context = Context::new();
        context.add_user_message(prompt);

        if let Some(max_tokens) = self.config.max_tokens {
            if context.total_tokens >= max_tokens {
                return Err(AgentError::MaxTokensExceeded {
                    max_tokens,
                    used_tokens: context.total_tokens,
                });
            }
        }

        let execution_result = self
            .executor
            .execute(&mut context, &self.provider, &self.tools, &self.config)
            .await
            .map_err(|execution_failure| AgentError::ExecutionFailed {
                error: execution_failure.error.into(),
                context: execution_failure.context,
            })?;

        let statistics = AgentRunStatistics::from_context(&context);

        Ok(AgentRunResult {
            output: execution_result.output,
            context,
            statistics,
        })
    }
}
