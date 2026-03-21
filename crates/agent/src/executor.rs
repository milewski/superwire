use crate::context::Context;
use crate::error::ExecutorError;
use crate::message::{ToolCall, ToolResult};
use crate::tool::ToolError;
use crate::tool::{FinalizeArguments, FinalizeOutput, FinalizeTool, RuntimeTool, Tool};
use crate::traits::{Executable, ExecutionResult, Provider, ProviderResponse, StopReason, ToolDefinition};
use crate::AgentConfig;
use async_trait::async_trait;
use futures::future::join_all;
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

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

    async fn process_finalize_tool_call(&self, context: &mut Context, tool_call: &ToolCall) -> Result<Option<O>, ExecutorError> {
        let input_result: Result<FinalizeArguments<O>, _> = serde_json::from_value(tool_call.arguments.clone());

        match input_result {
            Ok(finalize_arguments) => match finalize_arguments.output {
                FinalizeOutput::Success { output } => {
                    let value = serde_json::to_value(&output).map_err(|error| ExecutorError::FinalizeOutputSerializationFailed {
                        message: error.to_string(),
                    })?;

                    context.add_tool_result(ToolResult::Success {
                        tool_call_id: tool_call.id.clone(),
                        content: value,
                    });

                    Ok(Some(output))
                }
                FinalizeOutput::Failure { reason } => {
                    context.add_tool_result(ToolResult::Failure {
                        tool_call_id: tool_call.id.clone(),
                        content: Value::String(format!("Agent failed to complete the task: {reason}")),
                    });

                    Err(ExecutorError::FinalizeFailure { reason })
                }
            },
            Err(error) => {
                let tool_error = ToolError::new(format!("Failed to deserialize finalize tool arguments: {error}"))
                    .with_suggestion("Check that the arguments match the expected schema")
                    .with_context("error", Value::String(error.to_string()));

                context.add_tool_result(ToolResult::Failure {
                    tool_call_id: tool_call.id.clone(),
                    content: Value::String(tool_error.to_agent_message()),
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
        config: &AgentConfig,
    ) -> Result<ExecutionResult<Self::Output>, ExecutorError> {
        let mut local_context = context.clone();
        let (tools, registry) = self.prepare_tools(tools)?;
        let finalize_tool_name = self.finalize_tool.name().to_string();
        let finalize_completion_messages = [
            format!(
                "You must finish by calling '{finalize_tool_name}'. Critical rule: DO NOT return success unless you have a definitive, confident answer that fully satisfies the user's request. If you are missing information, unsure, blocked, or unable to complete any requirement, call '{finalize_tool_name}' with failure and include a clear reason describing what prevented completion."
            ),
            format!(
                "Quality gate: treat success as 'ready to ship'. If your answer is uncertain, partial, or speculative, it is not success. In that case call '{finalize_tool_name}' with failure and explain the uncertainty or blocker."
            ),
            format!(
                "Reliability rule: false-positive success is worse than failure. When confidence is not high enough to stand behind the final result, call '{finalize_tool_name}' with failure and provide the exact limitation."
            ),
            format!(
                "Decision rule: choose success only if every required part is completed correctly and you are confident it is accurate. Otherwise choose failure and call '{finalize_tool_name}' with a concrete reason."
            ),
            format!(
                "Final instruction for this turn: call '{finalize_tool_name}' now. If there is any doubt, incompleteness, or blocker, return failure. Return success only with a definitive answer."
            ),
        ];

        let mut iteration = 0;

        loop {
            // Stop runaway conversations once the iteration budget is exhausted
            if iteration >= self.max_iterations {
                return Err(ExecutorError::MaxIterationsReached {
                    max_iterations: self.max_iterations,
                });
            }

            // Ask the provider to extend the conversation using the current context and tools
            let response = provider
                .generate(&local_context, &tools, config)
                .await
                .map_err(|message| ExecutorError::ProviderFailed { iteration, message })?;

            // Preserve plain text replies alongside tool calls so the transcript stays coherent
            if let Some(text) = &response.text {
                let trimmed = text.trim_matches(|char| char == '\n' || char == '\r' || char == '\t' || char == ' ');

                if !trimmed.is_empty() {
                    local_context.add_assistant_message(trimmed);
                }
            }

            // Abort if the model repeats itself to avoid infinite loops
            if local_context.is_stuck(5) {
                break Err(ExecutorError::StuckLoopDetected)?;
            }

            // If the provider did not request any tool calls
            if response.tool_calls.is_empty() {
                // Nudge the model toward the finalize tool when it tries to stop without completing
                if response.stop_reason == StopReason::EndOfSequence {
                    let message_index = usize::min(iteration, finalize_completion_messages.len() - 1);
                    local_context.add_user_message(finalize_completion_messages[message_index].clone());
                }

                iteration += 1;

                continue;
            }

            // Persist every requested tool call before executing so the history remains authoritative
            for tool_call in &response.tool_calls {
                local_context.add_tool_call(tool_call.clone());
            }

            match self.classify_tool_calls(&response) {
                ToolCallExecution::Complete(finalize_tool_call) => {
                    if let Some(result) = self.process_finalize_tool_call(&mut local_context, finalize_tool_call).await? {
                        break Ok(ExecutionResult {
                            output: result,
                            context: local_context,
                        });
                    }
                }
                ToolCallExecution::Continue(tool_calls_to_execute) => {
                    // Run non-finalize tools concurrently to reduce overall latency
                    let tool_execution_futures = tool_calls_to_execute.into_iter().map(|tool_call| {
                        let tool = registry.get(&tool_call.name).expect("tool registry should contain every tool");

                        async move { (tool_call, tool.execute(tool_call.arguments.clone()).await) }
                    });

                    for (tool_call, tool_execution_result) in join_all(tool_execution_futures).await {
                        let tool_result = match tool_execution_result {
                            Ok(response) => ToolResult::Success {
                                tool_call_id: tool_call.id.clone(),
                                content: response,
                            },
                            Err(error) => ToolResult::Failure {
                                tool_call_id: tool_call.id.clone(),
                                content: error.to_agent_message().into(),
                            },
                        };

                        local_context.add_tool_result(tool_result);
                    }
                }
            }
        }
    }
}
