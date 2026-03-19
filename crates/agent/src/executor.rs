use crate::context::Context;
use crate::message::ToolResult;
use crate::tool::ToolError;
use crate::traits::{Executable, Provider, ProviderResponse, StopReason};
use async_trait::async_trait;
use schemars::schema_for;
use serde_json::json;

/// Executor that loops until a "done" tool is called with valid output
pub struct LoopExecutor<P, O>
where
    P: Provider,
    O: Send + Sync + 'static,
{
    max_iterations: usize,
    phantom: std::marker::PhantomData<(P, O)>,
}

impl<P, O> LoopExecutor<P, O>
where
    P: Provider + Send + Sync,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema + 'static,
{
    pub fn new() -> Self {
        Self {
            max_iterations: 5,
            phantom: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    async fn try_process_done_tool(
        &self,
        context: &mut Context,
        response: &ProviderResponse,
    ) -> Result<Option<String>, String> {
        let is_done_only = response.tool_calls.len() == 1 && response.tool_calls[0].name == "done";

        if !is_done_only {
            return Ok(None);
        }

        let tool_call = &response.tool_calls[0];
        let input_result: Result<O, _> = serde_json::from_value(tool_call.arguments.clone());

        match input_result {
            Ok(input) => {
                let value = serde_json::to_value(&input).map_err(|error| {
                    format!("Failed to serialize done tool output: {error}")
                })?;

                context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: value.clone(),
                    is_error: false,
                });

                let content = serde_json::to_string(&value).unwrap_or_else(|_| value.to_string());
                Ok(Some(content))
            }
            Err(error) => {
                let tool_error = ToolError::new(format!("Failed to deserialize done tool arguments: {error}"))
                    .with_suggestion("Check that the arguments match the expected schema".to_string())
                    .with_context("error".to_string(), serde_json::Value::String(error.to_string()));

                context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: serde_json::Value::String(tool_error.to_agent_message()),
                    is_error: true,
                });
                context.increment_attempt();

                Ok(None)
            }
        }
    }
}

#[async_trait]
impl<P, O> Executable for LoopExecutor<P, O>
where
    P: Provider + Send + Sync,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema,
{
    type Prompt = String;
    type Output = String;
    type Provider = P;

    async fn execute(
        &self,
        context: &Context,
        provider: &Self::Provider,
    ) -> Result<Self::Output, String> {
        let mut local_context = context.clone();

        let root_schema = schema_for!(O);
        local_context.done_tool_schema = Some(serde_json::to_value(&root_schema).unwrap_or_else(|_| json!({})));

        let mut iteration = 0;

        loop {
            if iteration >= self.max_iterations {
                return Err(format!(
                    "Maximum iterations ({}) reached without calling done tool",
                    self.max_iterations
                ));
            }

            let response = provider
                .generate(&local_context)
                .await
                .map_err(|error| format!("Provider error at iteration {iteration}: {error}"))?;

            if let Some(text) = &response.text {
                local_context.add_assistant_message(text.clone());
            }

            if response.tool_calls.is_empty() {
                if response.stop_reason == StopReason::EndOfSequence {
                    local_context.add_system_message(
                        "You must call the 'done' tool to complete the task. Do not end the conversation without calling this tool.".to_string()
                    );
                }

                iteration += 1;

                continue;
            }

            for tool_call in &response.tool_calls {
                local_context.add_tool_call(tool_call.clone());
            }

            if let Some(result) = self.try_process_done_tool(&mut local_context, &response).await? {
                return Ok(result);
            }

            iteration += 1;
        }
    }
}

