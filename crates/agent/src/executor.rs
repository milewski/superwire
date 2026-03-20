use crate::context::Context;
use crate::error::ExecutorError;
use crate::message::ToolResult;
use crate::tool::ToolError;
use crate::tool::{DoneArguments, DoneTool, RuntimeTool};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

type ToolRegistry<'a> = HashMap<String, &'a Arc<dyn RuntimeTool>>;

/// Executor that loops until a "done" tool is called with valid output
pub struct LoopExecutor<P, O>
where
    P: Provider,
    O: Send + Sync + 'static,
{
    max_iterations: usize,
    phantom: PhantomData<(P, O)>,
}

impl<P, O> LoopExecutor<P, O>
where
    P: Provider + Send + Sync,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema + 'static,
{
    pub fn new() -> Result<Self, ToolError> {
        Ok(Self {
            max_iterations: 5,
            phantom: PhantomData,
        })
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    fn prepare_tools<'a>(
        tools: &'a [Arc<dyn RuntimeTool>],
    ) -> Result<(Vec<ToolDefinition>, ToolRegistry<'a>), ExecutorError> {
        let mut definitions = Vec::with_capacity(tools.len() + 1);
        let mut registry = HashMap::with_capacity(tools.len());

        for tool in tools {
            let definition = tool.definition()?;
            registry.insert(definition.name.clone(), tool);
            definitions.push(definition);
        }

        // Inject done tool
        definitions.push(DoneTool::<O>::new()?.as_definition());

        Ok((definitions, registry))
    }

    async fn try_process_done_tool(
        context: &mut Context,
        response: &ProviderResponse,
    ) -> Result<Option<O>, ExecutorError> {
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
                    .map_err(|error| ExecutorError::new(format!("Failed to serialize done tool output: {error}")))?;

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
                    .with_context("error", Value::String(error.to_string()));

                context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: Value::String(tool_error.to_agent_message()),
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
    type Output = O;
    type Error = ExecutorError;
    type Provider = P;

    async fn execute(
        &self,
        context: &Context,
        provider: &Self::Provider,
        tools: &[Arc<dyn RuntimeTool>],
    ) -> Result<Self::Output, ExecutorError> {
        let mut local_context = context.clone();
        let (tool_definitions, registry) = Self::prepare_tools(tools)?;

        let mut iteration = 0;

        loop {
            // Fail fast if the agent exceeded its allowed iterations
            if iteration >= self.max_iterations {
                return Err(ExecutorError::new(format!(
                    "Maximum iterations ({}) reached without calling done tool",
                    self.max_iterations
                )));
            }

            // Ask the provider to generate a response given the current context and tools
            let response = provider
                .generate(&local_context, &tool_definitions)
                .await
                .map_err(|error| ExecutorError::new(format!("Provider error at iteration {iteration}: {error}")))?;

            // If the provider returned plain text, append it as an assistant message
            if let Some(text) = &response.text {
                local_context.add_assistant_message(text);
            }

            // If the provider did not request any tool calls
            if response.tool_calls.is_empty() {
                // Prompt the model to use the done tool if it tried to end the conversation
                if response.stop_reason == StopReason::EndOfSequence {
                    local_context.add_system_message(
                        "You must call the 'done' tool to complete the task. Do not end the conversation without calling this tool."
                    );
                }

                iteration += 1;

                continue;
            }

            // Record every tool call in the context so the conversation history is complete
            for tool_call in &response.tool_calls {
                local_context.add_tool_call(tool_call.clone());
            }

            // If the only tool call is done and its arguments are valid, return the result
            if let Some(result) = Self::try_process_done_tool(&mut local_context, &response).await? {
                return Ok(result);
            }

            // Execute every non-done tool call and record its result in the context
            for tool_call in &response.tool_calls {
                if tool_call.name == "done" {
                    continue;
                }

                let tool = registry
                    .get(&tool_call.name)
                    .expect("tool registry should contain every non-done tool");

                let (is_error, content) = match tool.execute(tool_call.arguments.clone()).await {
                    Ok(response) => (false, response),
                    Err(error) => (true, error.to_agent_message().into()),
                };

                local_context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content,
                    is_error,
                });
            }
        }
    }
}
