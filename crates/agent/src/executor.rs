use crate::context::Context;
use crate::error::ExecutorError;
use crate::message::{ToolCall, ToolResult};
use crate::tool::ToolError;
use crate::tool::{FinalizeArguments, FinalizeTool, RuntimeTool, Tool};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

type ToolRegistry<'a> = HashMap<String, &'a Arc<dyn RuntimeTool>>;

enum ToolCallExecution<'a> {
    Complete(&'a ToolCall),
    Continue(Vec<&'a ToolCall>),
}

/// Executor that loops until a "finalize" tool is called with valid output
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
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema + 'static,
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

    fn prepare_tools<'a>(
        &self,
        tools: &'a [Arc<dyn RuntimeTool>],
    ) -> Result<(Vec<ToolDefinition>, ToolRegistry<'a>), ExecutorError> {
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
        let mut non_finalize_tool_calls = Vec::new();

        for tool_call in &response.tool_calls {
            if tool_call.name == finalize_name {
                finalize_tool_call = Some(tool_call);
                continue;
            }

            non_finalize_tool_calls.push(tool_call);
        }

        if non_finalize_tool_calls.is_empty() {
            if let Some(finalize_tool_call) = finalize_tool_call {
                return ToolCallExecution::Complete(finalize_tool_call);
            }
        }

        ToolCallExecution::Continue(non_finalize_tool_calls)
    }

    async fn process_finalize_tool_call(
        &self,
        context: &mut Context,
        tool_call: &ToolCall,
    ) -> Result<Option<O>, ExecutorError> {
        let input_result: Result<FinalizeArguments<O>, _> = serde_json::from_value(tool_call.arguments.clone());

        match input_result {
            Ok(finalize_arguments) => {
                let output = finalize_arguments.output;
                let value = serde_json::to_value(&output).map_err(|error| {
                    ExecutorError::new(format!("Failed to serialize finalize tool output: {error}"))
                })?;

                context.add_tool_result(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: value,
                    is_error: false,
                });

                Ok(Some(output))
            }
            Err(error) => {
                let tool_error = ToolError::new(format!("Failed to deserialize finalize tool arguments: {error}"))
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
        let (tools, registry) = self.prepare_tools(tools)?;

        let mut iteration = 0;

        loop {
            // Fail fast if the agent exceeded its allowed iterations
            if iteration >= self.max_iterations {
                return Err(ExecutorError::new(format!(
                    "Maximum iterations ({}) reached without calling finalize tool",
                    self.max_iterations
                )));
            }

            // Ask the provider to generate a response given the current context and tools
            let response = provider
                .generate(&local_context, &tools)
                .await
                .map_err(|error| ExecutorError::new(format!("Provider error at iteration {iteration}: {error}")))?;

            // If the provider returned plain text, append it as an assistant message
            if let Some(text) = &response.text {
                local_context.add_assistant_message(text);
            }

            // Break if the agent is stuck in a loop
            if local_context.is_stuck(5) {
                break Err(ExecutorError::new("Agent is stuck in a repeated loop"))?;
            }

            // If the provider did not request any tool calls
            if response.tool_calls.is_empty() {
                // Prompt the model to use the finalize tool if it tried to end the conversation
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

            match self.classify_tool_calls(&response) {
                ToolCallExecution::Complete(finalize_tool_call) => {
                    if let Some(result) = self
                        .process_finalize_tool_call(&mut local_context, finalize_tool_call)
                        .await?
                    {
                        break Ok(result);
                    }
                }
                ToolCallExecution::Continue(tool_calls_to_execute) => {
                    for tool_call in tool_calls_to_execute {
                        let tool = registry
                            .get(&tool_call.name)
                            .expect("tool registry should contain every tool");

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
    }
}
