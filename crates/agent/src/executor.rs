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
    use crate::tool::ToolError;
    use serde::{Deserialize, Serialize};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    macro_rules! tool_call_json {
        (id = $identifier:expr, name = $tool_name:expr, output = $arguments:tt $(,)?) => {
            ToolCall {
                id: $identifier.to_string(),
                name: $tool_name.to_string(),
                arguments: serde_json::json!($arguments),
            }
        };
    }

    macro_rules! finalize_success_call {
        ($answer:tt) => {
            finalize_success_call!("finalize", $answer)
        };
        ($identifier:expr, $answer:tt) => {
            tool_call_json!(
                id = $identifier,
                name = "finalize",
                output = {
                    "output": {
                        "type": "success",
                        "answer": $answer,
                    }
                }
            )
        };
    }

    macro_rules! finalize_failure_call {
        ($reason:expr) => {
            finalize_failure_call!("finalize", $reason)
        };
        ($identifier:expr, $reason:expr) => {
            tool_call_json!(
                id = $identifier,
                name = "finalize",
                output = {
                    "output": {
                        "type": "failure",
                        "reason": $reason,
                    }
                }
            )
        };
    }

    macro_rules! provider_response {
        ($stop_reason:expr, [$($tool_call:expr),* $(,)?]) => {
            ProviderResponse {
                tool_calls: vec![$($tool_call),*],
                text: None,
                stop_reason: $stop_reason,
                usage: None,
            }
        };
    }

    macro_rules! provider {
        ($([$($tool_call:expr),* $(,)?]),+ $(,)?) => {
            MockProvider::from_results(vec![
                $(Ok(provider_response!(StopReason::ToolCalls, [$($tool_call),*]))),+
            ])
        };
    }

    macro_rules! run_executor {
        ($provider:expr => $output_type:ty) => {{
            let mut context = Context::default();
            let executor = LoopExecutor::<MockProvider, $output_type>::new().expect("executor should build");
            let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();
            let output = executor
                .execute(&mut context, &$provider, &runtime_tools, &AgentConfig::default())
                .await;

            (context, output)
        }};
        ($provider:expr => $output_type:ty, tools = [$($tool:expr),* $(,)?]) => {{
            let mut context = Context::default();
            let executor = LoopExecutor::<MockProvider, $output_type>::new().expect("executor should build");
            let runtime_tools: Vec<Arc<dyn RuntimeTool>> = vec![$(Arc::new($tool) as Arc<dyn RuntimeTool>),*];
            let output = executor
                .execute(&mut context, &$provider, &runtime_tools, &AgentConfig::default())
                .await;

            (context, output)
        }};
        ($provider:expr => $output_type:ty, max_iterations = $max_iterations:expr) => {{
            let mut context = Context::default();
            let executor = LoopExecutor::<MockProvider, $output_type>::new()
                .expect("executor should build")
                .with_max_iterations($max_iterations);
            let runtime_tools: Vec<Arc<dyn RuntimeTool>> = Vec::new();
            let output = executor
                .execute(&mut context, &$provider, &runtime_tools, &AgentConfig::default())
                .await;

            (context, output)
        }};
    }

    macro_rules! assert_tool_result {
        ($context:expr, $tool_call_id:expr) => {
            assert!(
                has_tool_result_for_call(&$context, $tool_call_id),
                "expected tool result for '{}'",
                $tool_call_id
            );
        };
    }

    macro_rules! assert_no_tool_result {
        ($context:expr, $tool_call_id:expr) => {
            assert!(
                !has_tool_result_for_call(&$context, $tool_call_id),
                "did not expect tool result for '{}'",
                $tool_call_id
            );
        };
    }

    macro_rules! assert_tool_failure_contains {
        ($context:expr, $tool_call_id:expr, [$($expected:expr),+ $(,)?]) => {{
            let failure_message = failure_message_for_tool_call(&$context, $tool_call_id)
                .expect("expected tool failure message");

            $(
                assert!(
                    failure_message.contains($expected),
                    "expected tool failure for '{}' to contain '{}'. got: {}",
                    $tool_call_id,
                    $expected,
                    failure_message
                );
            )+
        }};
    }

    #[derive(Debug)]
    struct MockProvider {
        queued_results: Mutex<VecDeque<Result<ProviderResponse, ProviderError>>>,
    }

    impl MockProvider {
        fn from_results(results: Vec<Result<ProviderResponse, ProviderError>>) -> Self {
            Self {
                queued_results: Mutex::new(VecDeque::from(results)),
            }
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn generate(
            &self,
            _context: &Context,
            _tools: &[ToolDefinition],
            _config: &AgentConfig,
        ) -> Result<ProviderResponse, ProviderError> {
            let mut queued_results = self.queued_results.lock().expect("mock provider queue lock should not be poisoned");

            queued_results
                .pop_front()
                .expect("mock provider should contain enough queued responses")
        }
    }

    #[derive(Debug, Clone, Default)]
    struct EchoTool;

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct EchoInput {
        value: String,
    }

    #[async_trait]
    impl Tool for EchoTool {
        type Input = EchoInput;

        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes a string back"
        }

        async fn execute(&self, input: Self::Input) -> Result<Value, ToolError> {
            Ok(serde_json::json!({ "echo": input.value }))
        }
    }

    fn failure_message_for_tool_call(context: &Context, tool_call_id: &str) -> Option<String> {
        for message in &context.messages {
            if let Message::ToolResult {
                result:
                    ToolResult::Failure {
                        tool_call_id: failure_tool_call_id,
                        content,
                    },
            } = message
            {
                if failure_tool_call_id == tool_call_id {
                    if let Some(content_text) = content.as_str() {
                        return Some(content_text.to_string());
                    }
                }
            }
        }

        None
    }

    fn has_tool_result_for_call(context: &Context, tool_call_id: &str) -> bool {
        for message in &context.messages {
            if let Message::ToolResult { result } = message {
                if result.tool_call_id() == tool_call_id {
                    return true;
                }
            }
        }

        false
    }

    #[tokio::test]
    async fn returns_output_when_finalize_success_is_valid() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([finalize_success_call!({ "age": 25, "name": "John Snow" })]);
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
    async fn retries_after_multiple_invalid_mixed_types_then_succeeds() {
        #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
        struct ValidationPayload {
            string: String,
            u8: u8,
            u16: u16,
            i8: i8,
            nullable: Option<String>,
            boolean: bool,
            float: f32,
            vec_string: Vec<String>,
            vec_u16: Vec<u16>,
            fixed_u8_3: [u8; 3],
        }

        #[rustfmt::skip]
        let provider = provider!(
            [finalize_success_call!("u8_type", { "string": "Alice", "u8": "30", "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("u16_max", { "string": "Alice", "u8": 30, "u16": 70000, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("i8_max", { "string": "Alice", "u8": 30, "u16": 500, "i8": 200, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("boolean_type", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": "true", "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("nullable_type", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": 123, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("vec_string_type", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", 1], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("vec_string_array_type", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": "a", "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("vec_u16_max", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 70000], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("fixed_u8_3_len", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3, 4] })],
            [finalize_success_call!("string_required", { "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })],
            [finalize_success_call!("valid", { "string": "Alice", "u8": 30, "u16": 500, "i8": 10, "nullable": null, "boolean": true, "float": 8.5, "vec_string": ["a", "b"], "vec_u16": [1, 2], "fixed_u8_3": [1, 2, 3] })]
        );

        let (context, output) = run_executor!(provider => ValidationPayload);

        assert_eq!(
            output.expect("execution should succeed after retry"),
            ValidationPayload {
                string: "Alice".to_string(),
                u8: 30,
                u16: 500,
                i8: 10,
                nullable: None,
                boolean: true,
                float: 8.5,
                vec_string: vec!["a".to_string(), "b".to_string()],
                vec_u16: vec![1, 2],
                fixed_u8_3: [1, 2, 3],
            }
        );

        assert_tool_failure_contains!(context, "u8_type", ["output.answer.u8", "integer"]);
        assert_tool_failure_contains!(context, "u16_max", ["output.answer.u16", "maximum"]);
        assert_tool_failure_contains!(context, "i8_max", ["output.answer.i8", "maximum"]);
        assert_tool_failure_contains!(context, "boolean_type", ["output.answer.boolean", "boolean"]);
        assert_tool_failure_contains!(context, "nullable_type", ["output.answer.nullable", "string"]);
        assert_tool_failure_contains!(context, "vec_string_type", ["output.answer.vec_string", "string"]);
        assert_tool_failure_contains!(context, "vec_string_array_type", ["output.answer.vec_string", "array"]);
        assert_tool_failure_contains!(context, "vec_u16_max", ["output.answer.vec_u16", "maximum"]);
        assert_tool_failure_contains!(context, "fixed_u8_3_len", ["output.answer.fixed_u8_3", "more than"]);
        assert_tool_failure_contains!(context, "string_required", ["output.answer.string is required"]);
    }

    #[tokio::test]
    async fn ignores_finalize_when_mixed_with_other_tools() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!(
            [
                finalize_success_call!({"name": "Ignored User", "age": 99}),
                tool_call_json!(id = "echo-1", name = "echo", output = {"value": "hello"}),
            ],
            [finalize_success_call!("finalize-final", {"name": "Maria", "age": 40})]
        );

        let (context, output) = run_executor!(provider => Person, tools = [EchoTool]);

        assert_eq!(
            output.expect("execution should succeed"),
            Person {
                name: "Maria".to_string(),
                age: 40,
            }
        );

        assert_no_tool_result!(context, "finalize");
        assert_tool_result!(context, "echo-1");
        assert_tool_result!(context, "finalize-final");
    }

    #[tokio::test]
    async fn returns_error_when_finalize_reports_failure() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([finalize_failure_call!("Not enough information")]);

        let (context, execution_error) = run_executor!(provider => Person);

        let execution_error = execution_error.expect_err("execution should fail");

        match execution_error {
            ExecutorError::FinalizeFailure { reason } => {
                assert_eq!(reason, "Not enough information");
            }
            unexpected_error => panic!("expected FinalizeFailure, got {unexpected_error:?}"),
        }

        assert_tool_failure_contains!(context, "finalize", ["Not enough information"]);
    }

    #[tokio::test]
    async fn returns_max_iterations_when_model_never_calls_tools() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
        struct Person {
            name: String,
            age: usize,
        }

        let provider = provider!([], []);

        let (_context, execution_error) = run_executor!(provider => Person, max_iterations = 2);

        let execution_error = execution_error.expect_err("execution should fail with iteration limit");

        match execution_error {
            ExecutorError::MaxIterationsReached { max_iterations } => {
                assert_eq!(max_iterations, 2);
            }
            unexpected_error => panic!("expected MaxIterationsReached, got {unexpected_error:?}"),
        }
    }

    #[tokio::test]
    async fn supports_custom_output_type_per_test() {
        #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
        struct Profile {
            city: String,
        }

        let provider = provider!([finalize_success_call!({"city": "Barcelona"})]);

        let (_, output) = run_executor!(provider => Profile);

        assert_eq!(
            output.expect("execution should succeed"),
            Profile {
                city: "Barcelona".to_string(),
            }
        );
    }
}
