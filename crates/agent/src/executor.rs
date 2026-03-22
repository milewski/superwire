use crate::context::Context;
use crate::error::{ExecutorError, ProviderError};
use crate::json_validation::validate_json_against_schema_with_context;
use crate::message::{ToolCall, ToolResult};
use crate::recovery_instruction::RecoveryInstruction;
use crate::tool::ToolError;
use crate::tool::{FinalizeArguments, FinalizeOutput, FinalizeTool, RuntimeTool, Tool};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
use crate::AgentConfig;
use async_trait::async_trait;
use futures::future::join_all;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

type ToolRegistry<'a> = HashMap<String, &'a Arc<dyn RuntimeTool>>;

enum ToolCallExecution<'a> {
    Complete(&'a ToolCall),
    Continue(Vec<&'a ToolCall>),
}

/// Drives a provider until the finalize tool returns a validated result
pub struct LoopExecutor<P, O>
where
    P: Provider,
    O: Send + Sync + 'static,
{
    max_iterations: usize,
    finalize_tool: FinalizeTool<O>,
    phantom: PhantomData<(P, O)>,
}

impl<P, O> LoopExecutor<P, O>
where
    P: Provider + Send + Sync,
    O: Send + Sync + Serialize + DeserializeOwned + JsonSchema + 'static,
{
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            max_iterations: 5,
            finalize_tool: FinalizeTool::<O>::new()?,
            phantom: PhantomData,
        })
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    fn prepare_tools<'a>(&self, tools: &'a [Arc<dyn RuntimeTool>]) -> Result<(Vec<ToolDefinition>, ToolRegistry<'a>), ExecutorError> {
        let mut definitions = Vec::with_capacity(tools.len() + 1);
        let mut registry = HashMap::with_capacity(tools.len());

        for tool in tools {
            let definition = tool.definition()?;
            registry.insert(definition.name.clone(), tool);
            definitions.push(definition);
        }

        definitions.push(self.finalize_tool.as_definition());

        Ok((definitions, registry))
    }

    fn classify_tool_calls<'a>(&self, response: &'a ProviderResponse) -> ToolCallExecution<'a> {
        let finalize_name = self.finalize_tool.name();
        let mut finalize_tool_call = None;
        let mut other_tool_calls = Vec::new();

        for tool_call in &response.tool_calls {
            if tool_call.name == finalize_name {
                finalize_tool_call = Some(tool_call);
                continue;
            }

            other_tool_calls.push(tool_call);
        }

        if other_tool_calls.is_empty() {
            if let Some(finalize_tool_call) = finalize_tool_call {
                return ToolCallExecution::Complete(finalize_tool_call);
            }
        }

        ToolCallExecution::Continue(other_tool_calls)
    }

    async fn process_finalize_tool_call(&self, context: &mut Context, tool_call: &ToolCall) -> Result<Option<O>, ExecutorError> {
        if let Err(error) = validate_json_against_schema_with_context(
            &tool_call.arguments,
            self.finalize_tool.parameters_schema(),
            "Finalize tool arguments do not match schema",
        ) {
            context.add_tool_result(ToolResult::Failure {
                tool_call_id: tool_call.id.clone(),
                content: Value::String(error.to_string()),
            });

            return Ok(None);
        }

        let input_result: Result<FinalizeArguments<O>, _> = serde_json::from_value(tool_call.arguments.clone());

        match input_result {
            Ok(arguments) => match arguments.output {
                FinalizeOutput::Success { answer } => {
                    context.add_tool_result(ToolResult::Success {
                        tool_call_id: tool_call.id.clone(),
                        content: serde_json::to_value(&answer).map_err(|error| ExecutorError::FinalizeOutputSerializationFailed {
                            message: error.to_string(),
                        })?,
                    });

                    Ok(Some(answer))
                }
                FinalizeOutput::Failure { reason } => {
                    context.add_tool_result(ToolResult::Failure {
                        tool_call_id: tool_call.id.clone(),
                        content: Value::String(reason.to_string()),
                    });

                    Err(ExecutorError::FinalizeFailure { reason })
                }
            },
            Err(error) => {
                context.add_tool_result(ToolResult::Failure {
                    tool_call_id: tool_call.id.clone(),
                    content: Value::String(error.to_string()),
                });

                Ok(None)
            }
        }
    }

    fn retry_delay_for_provider_error(&self, error: &ProviderError, attempt: usize, base_delay_ms: u64) -> Duration {
        if let ProviderError::RateLimited {
            message: _,
            retry_after_seconds: Some(retry_after_seconds),
        } = error
        {
            return Duration::from_secs(*retry_after_seconds);
        }

        let growth_factor = 2_u64.saturating_pow(attempt as u32);
        let delay_ms = base_delay_ms.saturating_mul(growth_factor);
        let capped_delay_ms = delay_ms.min(30_000);

        Duration::from_millis(capped_delay_ms)
    }

    async fn generate_with_retry(
        &self,
        context: &Context,
        provider: &P,
        tools: &[ToolDefinition],
        config: &AgentConfig,
    ) -> Result<ProviderResponse, ExecutorError> {
        let mut attempt = 0;

        loop {
            match provider.generate(context, tools, config).await {
                Ok(response) => break Ok(response),
                Err(error) if error.is_retriable() && attempt < config.provider_max_retries => {
                    sleep(self.retry_delay_for_provider_error(&error, attempt, config.provider_retry_base_delay_ms)).await;
                    attempt += 1;
                }
                Err(error) => {
                    break Err(ExecutorError::ProviderFailed { error });
                }
            }
        }
    }
}

#[async_trait]
impl<P, O> Executable for LoopExecutor<P, O>
where
    P: Provider + Send + Sync,
    O: Send + Sync + Serialize + DeserializeOwned + JsonSchema,
{
    type Output = O;
    type Error = ExecutorError;
    type Provider = P;

    async fn execute(
        &self,
        context: &mut Context,
        provider: &Self::Provider,
        tools: &[Arc<dyn RuntimeTool>],
        config: &AgentConfig,
    ) -> Result<Self::Output, ExecutorError> {
        let (tools, registry) = self.prepare_tools(tools)?;

        let mut iteration = 0;

        loop {
            // Stop runaway conversations once the iteration budget is exhausted
            if iteration >= self.max_iterations {
                return Err(ExecutorError::MaxIterationsReached {
                    max_iterations: self.max_iterations,
                });
            }

            // Ask the provider to extend the conversation using the current context and tools
            let response = self.generate_with_retry(context, provider, &tools, config).await?;

            if let Some(usage) = response.usage {
                context.add_token_usage(usage);
            }

            // Preserve plain text replies alongside tool calls so the transcript stays coherent
            if let Some(text) = &response.text {
                let trimmed = text.trim_matches(|char| char == '\n' || char == '\r' || char == '\t' || char == ' ');

                if !trimmed.is_empty() {
                    context.add_assistant_message(trimmed);
                }
            }

            // Abort if the model repeats itself to avoid infinite loops
            if context.is_stuck(config.stuck_threshold) {
                return Err(ExecutorError::StuckLoopDetected);
            }

            // Hard stop: token budget is exhausted, so retrying this loop cannot recover.
            if response.stop_reason == StopReason::MaxTokens {
                return Err(ExecutorError::MaxTokensReached);
            }

            // Nudge the model toward the finalize tool when it tries to stop without completing
            if response.stop_reason == StopReason::EndOfSequence {
                context.add_user_message(RecoveryInstruction::MustExitByCallingTool {
                    tool_name: self.finalize_tool.name(),
                });
            }

            // This executor is tool-driven: progress is only made through tool calls.
            // If the model replies without calling a tool, it has not executed any
            // actionable step toward completion, so the turn is treated as incomplete
            // and retried on the next iteration.
            if response.tool_calls.is_empty() {
                iteration += 1;
                continue;
            }

            // Persist every requested tool call before executing so the history remains authoritative
            for tool_call in &response.tool_calls {
                context.add_tool_call(tool_call.clone());
            }

            // Completion rule:
            // - If the model returns ONLY the finalize tool call, execution is complete.
            // - If finalize is mixed with other tool calls, finalize is ignored for this turn.
            // - If only non-finalize tools are returned, execute them and continue looping.
            // The loop ends only when finalize is requested by itself.
            match self.classify_tool_calls(&response) {
                ToolCallExecution::Complete(finalize_tool_call) => {
                    let output = match self.process_finalize_tool_call(context, finalize_tool_call).await {
                        Ok(output) => output,
                        Err(error) => break Err(error),
                    };

                    if let Some(result) = output {
                        break Ok(result);
                    }
                }
                ToolCallExecution::Continue(tool_calls) => {
                    // Run non-finalize tools concurrently to reduce overall latency
                    let tool_execution_futures = tool_calls.into_iter().map(|tool_call| {
                        let tool = registry.get(&tool_call.name).expect("tool registry should contain every tool");

                        async move { (tool_call, tool.execute(tool_call.arguments.clone()).await) }
                    });

                    for (tool_call, tool_execution_result) in join_all(tool_execution_futures).await {
                        let tool_result = match tool_execution_result {
                            Ok(content) => ToolResult::Success {
                                tool_call_id: tool_call.id.clone(),
                                content,
                            },
                            Err(error) => ToolResult::Failure {
                                tool_call_id: tool_call.id.clone(),
                                content: Value::String(error.to_string()),
                            },
                        };

                        context.add_tool_result(tool_result);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;
    use crate::tests::executor_support::{EchoTool, MockProvider};
    use crate::traits::TokenUsage;
    use crate::ProviderError;
    use crate::{
        assert_has_tool_success_content, assert_no_tool_result, assert_tool_failure_contains, assert_tool_result, assistant_message,
        provider, provider_error, run_executor, tool_call,
    };
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};

    #[tokio::test]
    async fn returns_output_when_finalize_success_is_valid() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([tool_call!(FinalizeTool::<Person>, { "age": 25, "name": "John Snow" })]);
        let (_, output) = run_executor!(provider => Person);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "John Snow".to_string(),
                age: 25,
            }
        );
    }

    #[tokio::test]
    async fn stores_only_trimmed_non_empty_assistant_text() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            assistant_message!(text = "\n\r\t   ", stop = StopReason::ToolCalls),
            assistant_message!(
                text = "\n  done with output  \t",
                stop = StopReason::ToolCalls,
                tools = [
                    tool_call!(FinalizeTool::<Person>, { "name": "John Snow", "age": 25 })
                ]
            )
        ]);

        let (context, output) = run_executor!(provider => Person);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "John Snow".to_string(),
                age: 25,
            }
        );

        let assistant_messages = context
            .messages
            .iter()
            .filter_map(|message| match message {
                Message::Assistant { content } => Some(content.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(assistant_messages, vec!["done with output".to_string()]);
    }

    #[tokio::test]
    async fn returns_stuck_loop_detected_for_repeated_assistant_messages() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([
            assistant_message!(text = "same message", stop = StopReason::ToolCalls),
            assistant_message!(text = "same message", stop = StopReason::ToolCalls)
        ]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new()
            .expect("executor should build")
            .with_max_iterations(10);

        let config = AgentConfig::default().with_stuck_threshold(2);
        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();

        let response = executor.execute(&mut context, &provider, &runtime_tools, &config).await;

        assert!(matches!(response, Err(ExecutorError::StuckLoopDetected)));
    }

    #[tokio::test]
    async fn returns_max_tokens_reached_when_provider_hits_token_limit() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            assistant_message!(text = "partial output", stop = StopReason::MaxTokens)
        ]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();

        let response = executor
            .execute(&mut context, &provider, &runtime_tools, &AgentConfig::default())
            .await;

        assert!(matches!(response, Err(ExecutorError::MaxTokensReached)));
    }

    #[tokio::test]
    async fn records_token_usage_even_when_max_tokens_reached() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([assistant_message!(
            text = "partial output",
            stop = StopReason::MaxTokens,
            usage = TokenUsage {
                total_tokens: 12,
                input_tokens: 5,
                output_tokens: 7,
            }
        )]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();

        let response = executor
            .execute(&mut context, &provider, &runtime_tools, &AgentConfig::default())
            .await;

        assert!(matches!(response, Err(ExecutorError::MaxTokensReached)));
        assert_eq!(context.total_tokens, 12);
        assert_eq!(context.input_tokens, 5);
        assert_eq!(context.output_tokens, 7);
    }

    #[tokio::test]
    async fn retries_retriable_provider_error_and_then_completes() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            provider_error!(ProviderError::Network {
                message: "temporary network issue".to_string(),
            }),
            assistant_message!(
                stop = StopReason::ToolCalls,
                tools = [
                    tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })
                ]
            )
        ]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let config = AgentConfig::default()
            .with_provider_max_retries(1)
            .with_provider_retry_base_delay_ms(0);

        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();
        let response = executor.execute(&mut context, &provider, &runtime_tools, &config).await;

        assert_eq!(
            response.expect("execution should succeed after provider retry"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );
    }

    #[tokio::test]
    async fn retries_rate_limited_provider_error_and_then_completes() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([
            provider_error!(ProviderError::RateLimited {
                message: "rate limited".to_string(),
                retry_after_seconds: Some(0),
            }),
            assistant_message!(
                stop = StopReason::ToolCalls,
                tools = [tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })]
            )
        ]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let config = AgentConfig::default()
            .with_provider_max_retries(1)
            .with_provider_retry_base_delay_ms(0);
        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();

        let response = executor.execute(&mut context, &provider, &runtime_tools, &config).await;

        assert_eq!(
            response.expect("execution should succeed after rate limit retry"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );
    }

    #[tokio::test]
    async fn returns_provider_failed_immediately_for_non_retriable_error() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            provider_error!(ProviderError::AuthenticationFailed {
                message: "invalid api key".to_string(),
            })
        ]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let config = AgentConfig::default()
            .with_provider_max_retries(5)
            .with_provider_retry_base_delay_ms(0);

        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();
        let response = executor.execute(&mut context, &provider, &runtime_tools, &config).await;

        assert!(matches!(
            response,
            Err(ExecutorError::ProviderFailed {
                error: ProviderError::AuthenticationFailed { .. }
            })
        ));
    }

    #[tokio::test]
    async fn returns_provider_failed_when_retries_are_exhausted() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([
            provider_error!(ProviderError::ServiceUnavailable {
                message: "provider down".to_string(),
            }),
            provider_error!(ProviderError::ServiceUnavailable {
                message: "provider still down".to_string(),
            })
        ]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let config = AgentConfig::default()
            .with_provider_max_retries(1)
            .with_provider_retry_base_delay_ms(0);

        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();
        let response = executor.execute(&mut context, &provider, &runtime_tools, &config).await;

        assert!(matches!(
            response,
            Err(ExecutorError::ProviderFailed {
                error: ProviderError::ServiceUnavailable { .. }
            })
        ));
    }

    #[tokio::test]
    async fn accumulates_provider_token_usage_across_iterations() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([
            assistant_message!(
                text = "thinking",
                stop = StopReason::ToolCalls,
                usage = TokenUsage {
                    total_tokens: 10,
                    input_tokens: 4,
                    output_tokens: 6,
                }
            ),
            assistant_message!(
                stop = StopReason::ToolCalls,
                tools = [tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })],
                usage = TokenUsage {
                    total_tokens: 7,
                    input_tokens: 2,
                    output_tokens: 5,
                }
            )
        ]);

        let (context, output) = run_executor!(provider => Person);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_eq!(context.total_tokens, 17);
        assert_eq!(context.input_tokens, 6);
        assert_eq!(context.output_tokens, 11);
    }

    #[tokio::test]
    async fn records_runtime_tool_input_deserialization_failure_and_recovers() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!(
            [tool_call!(EchoTool, id = "echo-invalid", { "value": 123 })],
            [tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })]
        );

        let (context, output) = run_executor!(provider => Person, tools = [EchoTool]);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_tool_failure_contains!(context, "echo-invalid", ["Failed to deserialize tool input for 'echo'"]);
        assert_tool_result!(context, "final");
    }

    #[tokio::test]
    async fn returns_tool_error_when_runtime_tool_definition_fails() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[derive(Debug)]
        struct BrokenDefinitionTool;

        #[async_trait]
        impl RuntimeTool for BrokenDefinitionTool {
            fn definition(&self) -> Result<ToolDefinition, ToolError> {
                Err(ToolError::new("broken tool definition"))
            }

            async fn execute(&self, _input: Value) -> Result<Value, ToolError> {
                Ok(json!({ "unused": true }))
            }
        }

        let provider = provider!([assistant_message!(text = "unused", stop = StopReason::ToolCalls)]);

        let mut context = Context::default();
        let executor = LoopExecutor::<MockProvider, Person>::new().expect("executor should build");
        let runtime_tools: Vec<Arc<dyn RuntimeTool>> = vec![Arc::new(BrokenDefinitionTool)];

        let response = executor
            .execute(&mut context, &provider, &runtime_tools, &AgentConfig::default())
            .await;

        assert!(matches!(
            response,
            Err(ExecutorError::ToolError {
                message,
                details: _
            }) if message == "broken tool definition"
        ));
    }

    #[tokio::test]
    async fn records_runtime_tool_failure_and_then_recovers_with_finalize() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[derive(Debug, Default, Clone)]
        struct AlwaysFailTool;

        #[derive(Debug, Serialize, Deserialize, JsonSchema)]
        struct AlwaysFailInput {
            value: String,
        }

        #[async_trait]
        impl Tool for AlwaysFailTool {
            type Input = AlwaysFailInput;

            fn name(&self) -> &str {
                "always_fail"
            }

            fn description(&self) -> &str {
                "Always fails for testing"
            }

            async fn execute(&self, _input: Self::Input) -> Result<Value, ToolError> {
                Err(ToolError::new("boom"))
            }
        }

        let provider = provider!(
            [tool_call!(AlwaysFailTool, id = "fail", { "value": "x" })],
            [tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })]
        );

        let (context, output) = run_executor!(provider => Person, tools = [AlwaysFailTool]);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_tool_failure_contains!(context, "fail", ["boom"]);
        assert_tool_result!(context, "final");
    }

    #[tokio::test]
    async fn adds_recovery_instruction_after_end_of_sequence_without_finalize() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            assistant_message!(text = "stopping early", stop = StopReason::EndOfSequence),
            tool_call!(FinalizeTool::<Person>, { "name": "Maria", "age": 40 })
        ]);

        let (context, output) = run_executor!(provider => Person);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert!(context.messages.iter().any(|message| {
            matches!(
                message,
                Message::User { content } if content.contains("You must finish by calling 'finalize'.")
            )
        }));
    }

    #[tokio::test]
    async fn supports_script_style_provider_sequence_with_mixed_response_types() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            assistant_message!(text = "   preparing final answer   ", stop = StopReason::ToolCalls),
            tool_call!(FinalizeTool::<Person>, id = "final", { "name": "Maria", "age": 40 })
        ]);

        let (context, output) = run_executor!(provider => Person);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        let assistant_messages = context
            .messages
            .iter()
            .filter_map(|message| match message {
                Message::Assistant { content } => Some(content.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(assistant_messages, vec!["preparing final answer".to_string()]);
    }

    #[tokio::test]
    async fn retries_after_multiple_invalid_mixed_types_then_succeeds() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct ValidationPayload {
            string: String,
            u8: u8,
            u16: u16,
            u32: u32,
            u64: u64,
            usize: usize,
            i8: i8,
            i16: i16,
            i32: i32,
            i64: i64,
            isize: isize,
            nullable: Option<String>,
            boolean: bool,
            float: f32,
            vec_string: Vec<String>,
            vec_u16: Vec<u16>,
            fixed_u8_3: [u8; 3],
            mixed_tuple: (String, u8, f32, bool, Option<String>),
        }

        fn finalize_success_case_with_defaults(case_id: &str, answer_patch: Value, keys_to_remove: &[&str]) -> ToolCall {
            let mut answer = json!({
                "string": "Alice",
                "u8": 30,
                "u16": 500,
                "u32": 500,
                "u64": 500,
                "usize": 500,
                "i8": 10,
                "i16": 10,
                "i32": 10,
                "i64": 10,
                "isize": 10,
                "nullable": null,
                "boolean": true,
                "float": 8.5,
                "vec_string": ["a", "b"],
                "vec_u16": [1, 2],
                "fixed_u8_3": [1, 2, 3],
                "mixed_tuple": ["hello", 7, 1.5, true, null]
            });

            if let (Some(answer_object), Some(answer_patch_object)) = (answer.as_object_mut(), answer_patch.as_object()) {
                for (key, value) in answer_patch_object {
                    answer_object.insert(key.clone(), value.clone());
                }

                for key_to_remove in keys_to_remove {
                    answer_object.remove(*key_to_remove);
                }
            }

            ToolCall {
                id: case_id.to_string(),
                name: "finalize".to_string(),
                arguments: json!({
                    "output": {
                        "type": "success",
                        "answer": answer,
                    }
                }),
            }
        }

        macro_rules! case_with_defaults {
            ($case_id:expr, $answer_patch:tt) => {
                finalize_success_case_with_defaults($case_id, json!($answer_patch), &[])
            };
            ($case_id:expr, $answer_patch:tt, remove = [$($key_to_remove:expr),* $(,)?]) => {
                finalize_success_case_with_defaults($case_id, json!($answer_patch), &[$($key_to_remove),*])
            };
        }

        #[rustfmt::skip]
        let provider = provider!(
            [case_with_defaults!("u8_type", { "u8": "30" })],
            [case_with_defaults!("u16_max", { "u16": 70000 })],
            [case_with_defaults!("u32_max", { "u32": 5000000000u64 })],
            [case_with_defaults!("u64_type", { "u64": "500" })],
            [case_with_defaults!("usize_min", { "usize": -1 })],
            [case_with_defaults!("i8_max", { "i8": 200 })],
            [case_with_defaults!("i16_max", { "i16": 40000 })],
            [case_with_defaults!("i32_max", { "i32": 3000000000i64 })],
            [case_with_defaults!("i64_type", { "i64": "10" })],
            [case_with_defaults!("isize_max", { "isize": 9223372036854775808u64 })],
            [case_with_defaults!("boolean_type", { "boolean": "true" })],
            [case_with_defaults!("nullable_type", { "nullable": 123 })],
            [case_with_defaults!("vec_string_type", { "vec_string": ["a", 1] })],
            [case_with_defaults!("vec_string_array_type", { "vec_string": "a" })],
            [case_with_defaults!("vec_u16_max", { "vec_u16": [1, 70000] })],
            [case_with_defaults!("fixed_u8_3_len", { "fixed_u8_3": [1, 2, 3, 4] })],
            [case_with_defaults!("mixed_tuple_type", { "mixed_tuple": ["hello", "7", 1.5, true, null] })],
            [case_with_defaults!("string_required", {}, remove = ["string"])],
            [case_with_defaults!("valid", {})]
        );

        let (context, output) = run_executor!(provider => ValidationPayload);

        assert_eq!(
            output.expect("execution should succeed after retry"),
            ValidationPayload {
                string: "Alice".to_string(),
                u8: 30,
                u16: 500,
                u32: 500,
                u64: 500,
                usize: 500,
                i8: 10,
                i16: 10,
                i32: 10,
                i64: 10,
                isize: 10,
                nullable: None,
                boolean: true,
                float: 8.5,
                vec_string: vec!["a".to_string(), "b".to_string()],
                vec_u16: vec![1u16, 2],
                fixed_u8_3: [1u8, 2, 3],
                mixed_tuple: ("hello".to_string(), 7, 1.5, true, None),
            }
        );

        assert_tool_failure_contains!(context, "u8_type", ["output.answer.u8", "integer"]);
        assert_tool_failure_contains!(context, "u16_max", ["output.answer.u16", "maximum"]);
        assert_tool_failure_contains!(context, "u32_max", ["expected u32"]);
        assert_tool_failure_contains!(context, "u64_type", ["output.answer.u64", "integer"]);
        assert_tool_failure_contains!(context, "usize_min", ["output.answer.usize", "minimum"]);
        assert_tool_failure_contains!(context, "i8_max", ["output.answer.i8", "maximum"]);
        assert_tool_failure_contains!(context, "i16_max", ["output.answer.i16", "maximum"]);
        assert_tool_failure_contains!(context, "i32_max", ["expected i32"]);
        assert_tool_failure_contains!(context, "i64_type", ["output.answer.i64", "integer"]);
        assert_tool_failure_contains!(context, "isize_max", ["expected isize"]);
        assert_tool_failure_contains!(context, "boolean_type", ["output.answer.boolean", "boolean"]);
        assert_tool_failure_contains!(context, "nullable_type", ["output.answer.nullable", "string"]);
        assert_tool_failure_contains!(context, "vec_string_type", ["output.answer.vec_string", "string"]);
        assert_tool_failure_contains!(context, "vec_string_array_type", ["output.answer.vec_string", "array"]);
        assert_tool_failure_contains!(context, "vec_u16_max", ["output.answer.vec_u16", "maximum"]);
        assert_tool_failure_contains!(context, "fixed_u8_3_len", ["output.answer.fixed_u8_3", "more than"]);
        assert_tool_failure_contains!(context, "mixed_tuple_type", ["output.answer.mixed_tuple", "integer"]);
        assert_tool_failure_contains!(context, "string_required", ["output.answer.string is required"]);
    }

    #[tokio::test]
    async fn ignores_finalize_when_mixed_with_other_tools() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: u8,
        }

        #[rustfmt::skip]
        let provider = provider!(
            [
                tool_call!(FinalizeTool<Person>, id = "a", { "name": "Ignored User", "age": 99 }),
                tool_call!(EchoTool, { "value": "hello" }),
            ],
            [
                tool_call!(FinalizeTool<Person>, id = "b", { "name": "Maria", "age": 40 })
            ]
        );

        let (context, output) = run_executor!(provider => Person, tools = [EchoTool]);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_no_tool_result!(context, "a");
        assert_has_tool_success_content!(context, { "echo": "hello" });
        assert_tool_result!(context, "b");
    }

    #[tokio::test]
    async fn returns_error_when_finalize_reports_failure() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        #[rustfmt::skip]
        let provider = provider!([
            tool_call!(FinalizeTool::<Person>, id = "finalize", failure = "Not enough information")
        ]);

        let (context, response) = run_executor!(provider => Person);

        match response.expect_err("execution should fail") {
            ExecutorError::FinalizeFailure { reason } => {
                assert_eq!(reason, "Not enough information");
            }
            error => panic!("expected FinalizeFailure, got {error:?}"),
        }

        assert_tool_failure_contains!(context, "finalize", ["Not enough information"]);
    }

    #[tokio::test]
    async fn returns_max_iterations_when_model_never_calls_tools() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([], []);
        let (_, response) = run_executor!(provider => Person, max_iterations = 2);

        match response.expect_err("execution should fail with iteration limit") {
            ExecutorError::MaxIterationsReached { max_iterations } => {
                assert_eq!(max_iterations, 2);
            }
            error => panic!("expected MaxIterationsReached, got {error:?}"),
        }
    }
}
