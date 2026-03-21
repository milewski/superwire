use crate::context::Context;
use crate::error::ExecutorError;
use crate::message::{ToolCall, ToolResult};
use crate::recovery_instruction::RecoveryInstruction;
use crate::tool::ToolError;
use crate::tool::{FinalizeArguments, FinalizeOutput, FinalizeTool, RuntimeTool, Tool};
use crate::traits::{Executable, Provider, ProviderResponse, StopReason, ToolDefinition};
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
            let response = provider
                .generate(context, &tools, config)
                .await
                .map_err(|message| ExecutorError::ProviderFailed { iteration, message })?;

            // Preserve plain text replies alongside tool calls so the transcript stays coherent
            if let Some(text) = &response.text {
                let trimmed = text.trim_matches(|char| char == '\n' || char == '\r' || char == '\t' || char == ' ');

                if !trimmed.is_empty() {
                    context.add_assistant_message(trimmed);
                }
            }

            // Abort if the model repeats itself to avoid infinite loops
            if context.is_stuck(5) {
                return Err(ExecutorError::StuckLoopDetected);
            }

            println!("{:?}", response.stop_reason);

            // If the provider did not request any tool calls
            if response.tool_calls.is_empty() {
                // Nudge the model toward the finalize tool when it tries to stop without completing
                if response.stop_reason == StopReason::EndOfSequence {
                    context.add_user_message(RecoveryInstruction::MustExitByCallingTool {
                        iteration,
                        tool_name: self.finalize_tool.name(),
                    });
                }

                iteration += 1;

                continue;
            }

            // Persist every requested tool call before executing so the history remains authoritative
            for tool_call in &response.tool_calls {
                context.add_tool_call(tool_call.clone());
            }

            match self.classify_tool_calls(&response) {
                ToolCallExecution::Complete(finalize_tool_call) => {
                    let output = match self.process_finalize_tool_call(context, finalize_tool_call).await {
                        Ok(output) => output,
                        Err(error) => return Err(error),
                    };

                    if let Some(result) = output {
                        break Ok(result);
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

                        context.add_tool_result(tool_result);
                    }
                }
            }
        }
    }
}
