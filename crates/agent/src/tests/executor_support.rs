use crate::agent::AgentConfig;
use crate::context::Context;
use crate::error::{ExecutorError, ProviderError};
use crate::message::{Message, ToolCall, ToolResult};
use crate::tool::{FinalizeTool, RuntimeTool, Tool, ToolError};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub fn build_provider_response(tool_calls: Vec<ToolCall>) -> ProviderResponse {
    ProviderResponse {
        tool_calls,
        text: None,
        stop_reason: StopReason::ToolCalls,
        usage: None,
    }
}

pub trait ToolCallFactory {
    fn build_success_tool_call(identifier: String, arguments: Value) -> ToolCall;

    fn build_failure_tool_call(_identifier: String, _reason: String) -> ToolCall {
        panic!("failure tool calls are only supported for finalize tool")
    }
}

pub fn build_tool_call<ToolType>(identifier: String, arguments: Value) -> ToolCall
where
    ToolType: ToolCallFactory,
{
    ToolType::build_success_tool_call(identifier, arguments)
}

pub fn build_tool_failure_call<ToolType>(identifier: String, reason: String) -> ToolCall
where
    ToolType: ToolCallFactory,
{
    ToolType::build_failure_tool_call(identifier, reason)
}

impl<ToolType> ToolCallFactory for ToolType
where
    ToolType: Tool + Default,
{
    fn build_success_tool_call(identifier: String, arguments: Value) -> ToolCall {
        ToolCall {
            id: identifier,
            name: ToolType::default().name().to_string(),
            arguments,
        }
    }
}

impl<OutputType> ToolCallFactory for FinalizeTool<OutputType>
where
    OutputType: Send + Sync + Serialize + DeserializeOwned + JsonSchema,
{
    fn build_success_tool_call(identifier: String, arguments: Value) -> ToolCall {
        let finalize_tool = FinalizeTool::<OutputType>::new().expect("finalize tool should build");

        ToolCall {
            id: identifier,
            name: finalize_tool.name().to_string(),
            arguments: serde_json::json!({
                "output": {
                    "type": "success",
                    "answer": arguments,
                }
            }),
        }
    }

    fn build_failure_tool_call(identifier: String, reason: String) -> ToolCall {
        let finalize_tool = FinalizeTool::<OutputType>::new().expect("finalize tool should build");

        ToolCall {
            id: identifier,
            name: finalize_tool.name().to_string(),
            arguments: serde_json::json!({
                "output": {
                    "type": "failure",
                    "reason": reason,
                }
            }),
        }
    }
}

#[derive(Debug)]
pub struct MockProvider {
    queued_results: Mutex<VecDeque<Result<ProviderResponse, ProviderError>>>,
}

impl MockProvider {
    pub fn from_results(results: Vec<Result<ProviderResponse, ProviderError>>) -> Self {
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
pub struct EchoTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EchoInput {
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

pub fn failure_message_for_tool_call(context: &Context, tool_call_id: &str) -> Option<String> {
    for message in &context.messages {
        if let Message::ToolResult {
            result: ToolResult::Failure {
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

pub fn has_tool_result_for_call(context: &Context, tool_call_id: &str) -> bool {
    for message in &context.messages {
        if let Message::ToolResult { result } = message {
            if result.tool_call_id() == tool_call_id {
                return true;
            }
        }
    }

    false
}

pub fn has_tool_success_content(context: &Context, expected_content: &Value) -> bool {
    for message in &context.messages {
        if let Message::ToolResult {
            result: ToolResult::Success { content, .. },
        } = message
        {
            if content == expected_content {
                return true;
            }
        }
    }

    false
}

pub async fn run_executor<OutputType>(
    provider: &MockProvider,
    runtime_tools: Vec<Arc<dyn RuntimeTool>>,
    max_iterations: Option<usize>,
) -> (Context, Result<OutputType, ExecutorError>)
where
    OutputType: Send + Sync + Serialize + DeserializeOwned + JsonSchema + 'static,
{
    let mut context = Context::default();
    let mut executor = crate::LoopExecutor::<MockProvider, OutputType>::new().expect("executor should build");

    if let Some(max_iterations) = max_iterations {
        executor = executor.with_max_iterations(max_iterations);
    }

    let output = executor
        .execute(&mut context, provider, &runtime_tools, &AgentConfig::default())
        .await;

    (context, output)
}

#[macro_export]
macro_rules! tool_call {
    ($tool_type:ty, failure = $reason:expr) => {
        $crate::tests::executor_support::build_tool_failure_call::<$tool_type>(
            format!("{}-{}", stringify!($tool_type), line!()),
            $reason.to_string(),
        )
    };
    ($tool_type:ty, id = $identifier:expr, failure = $reason:expr) => {
        $crate::tests::executor_support::build_tool_failure_call::<$tool_type>($identifier.to_string(), $reason.to_string())
    };
    ($tool_type:ty, $arguments:tt) => {
        $crate::tests::executor_support::build_tool_call::<$tool_type>(
            format!("{}-{}", stringify!($tool_type), line!()),
            serde_json::json!($arguments),
        )
    };
    ($tool_type:ty, id = $identifier:expr, $arguments:tt) => {
        $crate::tests::executor_support::build_tool_call::<$tool_type>($identifier.to_string(), serde_json::json!($arguments))
    };
}

#[macro_export]
macro_rules! provider {
    ($([$($tool_call:expr),* $(,)?]),+ $(,)?) => {
        $crate::tests::executor_support::MockProvider::from_results(vec![
            $(Ok($crate::tests::executor_support::build_provider_response(vec![$($tool_call),*]))),+
        ])
    };
}

#[macro_export]
macro_rules! run_executor {
    ($provider:expr => $output_type:ty) => {{
        $crate::tests::executor_support::run_executor::<$output_type>(&$provider, Vec::new(), None).await
    }};
    ($provider:expr => $output_type:ty, tools = [$($tool:expr),* $(,)?]) => {{
        let runtime_tools: Vec<std::sync::Arc<dyn $crate::RuntimeTool>> = vec![$(std::sync::Arc::new($tool) as std::sync::Arc<dyn $crate::RuntimeTool>),*];
        $crate::tests::executor_support::run_executor::<$output_type>(&$provider, runtime_tools, None).await
    }};
    ($provider:expr => $output_type:ty, max_iterations = $max_iterations:expr) => {{
        $crate::tests::executor_support::run_executor::<$output_type>(&$provider, Vec::new(), Some($max_iterations)).await
    }};
}

#[macro_export]
macro_rules! assert_tool_result {
    ($context:expr, $tool_call_id:expr) => {
        assert!(
            $crate::tests::executor_support::has_tool_result_for_call(&$context, $tool_call_id),
            "expected tool result for '{}'",
            $tool_call_id
        );
    };
}

#[macro_export]
macro_rules! assert_no_tool_result {
    ($context:expr, $tool_call_id:expr) => {
        assert!(
            !$crate::tests::executor_support::has_tool_result_for_call(&$context, $tool_call_id),
            "did not expect tool result for '{}'",
            $tool_call_id
        );
    };
}

#[macro_export]
macro_rules! assert_tool_failure_contains {
    ($context:expr, $tool_call_id:expr, [$($expected:expr),+ $(,)?]) => {{
        let failure_message = $crate::tests::executor_support::failure_message_for_tool_call(&$context, $tool_call_id)
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

#[macro_export]
macro_rules! assert_has_tool_success_content {
    ($context:expr, $expected_content:tt) => {
        assert!(
            $crate::tests::executor_support::has_tool_success_content(&$context, &serde_json::json!($expected_content)),
            "expected tool success content: {}",
            serde_json::json!($expected_content)
        );
    };
}
