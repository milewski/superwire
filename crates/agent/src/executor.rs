use crate::context::Context;
use crate::message::ToolResult;
use crate::tool::ToolError;
use crate::tool::{DoneArguments, DoneTool, RuntimeTool};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
use async_trait::async_trait;
use std::sync::Arc;

/// Executor that loops until a "done" tool is called with valid output
pub struct LoopExecutor<P, O>
where
    P: Provider,
    O: Send + Sync + 'static,
{
    max_iterations: usize,
    done_tool: DoneTool<O>,
    phantom: std::marker::PhantomData<P>,
}

impl<P, O> LoopExecutor<P, O>
where
    P: Provider + Send + Sync,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema + 'static,
{
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            max_iterations: 5,
            done_tool: DoneTool::new()?,
            phantom: std::marker::PhantomData,
        })
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    fn build_tools(&self, tools: &[Arc<dyn RuntimeTool>]) -> Result<Vec<ToolDefinition>, String> {
        let mut tool_definitions = Vec::with_capacity(tools.len() + 1);

        for tool in tools {
            tool_definitions.push(tool.definition().map_err(|error| error.to_string())?);
        }

        tool_definitions.push(self.done_tool.as_definition());

        Ok(tool_definitions)
    }

    async fn try_process_done_tool(
        &self,
        context: &mut Context,
        response: &ProviderResponse,
    ) -> Result<Option<O>, String> {
        let is_done_only = response.tool_calls.len() == 1 && response.tool_calls[0].name == "done";

        if !is_done_only {
            return Ok(None);
        }

        let tool_call = &response.tool_calls[0];
        let input_result: Result<DoneArguments<O>, _> = serde_json::from_value(tool_call.arguments.clone());

        match input_result {
            Ok(done_arguments) => {
                let output = done_arguments.output;
                let value = serde_json::to_value(&output)
                    .map_err(|error| format!("Failed to serialize done tool output: {error}"))?;

                context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: value,
                    is_error: false,
                });

                Ok(Some(output))
            }
            Err(error) => {
                let tool_error = ToolError::new(format!("Failed to deserialize done tool arguments: {error}"))
                    .with_suggestion("Check that the arguments match the expected schema")
                    .with_context("error", serde_json::Value::String(error.to_string()));

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
    type Output = O;
    type Provider = P;

    async fn execute(
        &self,
        context: &Context,
        provider: &Self::Provider,
        tools: &[Arc<dyn RuntimeTool>],
    ) -> Result<Self::Output, String> {
        let mut local_context = context.clone();
        let tool_definitions = self.build_tools(tools)?;

        let mut iteration = 0;

        loop {
            if iteration >= self.max_iterations {
                return Err(format!(
                    "Maximum iterations ({}) reached without calling done tool",
                    self.max_iterations
                ));
            }

            let response = provider
                .generate(&local_context, &tool_definitions)
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

            for tool_call in &response.tool_calls {
                if tool_call.name == "done" {
                    continue;
                }

                let Some(tool) = tools.iter().find(|tool| {
                    tool.definition()
                        .map(|definition| definition.name == tool_call.name)
                        .unwrap_or(false)
                }) else {
                    let error_message = format!("Unknown tool '{}'", tool_call.name);

                    local_context.add_tool_result(ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: serde_json::Value::String(error_message),
                        is_error: true,
                    });

                    continue;
                };

                let (is_error, tool_call_id, content) = match tool.execute_json(tool_call.arguments.clone()).await {
                    Ok(result) => (false, tool_call.id.clone(), result),
                    Err(error) => (
                        true,
                        tool_call.id.clone(),
                        serde_json::Value::String(error.to_agent_message()),
                    ),
                };

                local_context.add_tool_result(ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                });
            }

            iteration += 1;
        }
    }
}
