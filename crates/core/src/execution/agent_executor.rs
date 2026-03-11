use crate::ast::Agent;
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::execution::orchestrator::AgentOrchestrator;
use crate::providers::provider::{Message, ToolDefinition};
use crate::schemas::validator::SchemaValidator;
use crate::tools::done::{DoneStatus, DoneTool};
use serde_json::Value as JsonValue;
use std::sync::Arc;

macro_rules! system {
    ($($content:tt)*) => {{
        let text = stringify!($($content)*);
        let normalized = text
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        Message::System {
            content: normalized,
        }
    }};
}

const MAX_ITERATIONS: usize = 50;

pub struct AgentExecutor<'a> {
    orchestrator: &'a AgentOrchestrator,
    agent: &'a Agent,
    runtime_context: &'a RuntimeContext,
}

impl<'a> AgentExecutor<'a> {
    #[must_use]
    pub fn new(orchestrator: &'a AgentOrchestrator, agent: &'a Agent, runtime_context: &'a RuntimeContext) -> Self {
        Self {
            orchestrator,
            agent,
            runtime_context,
        }
    }

    pub async fn execute(&self, initial_context: Vec<Message>) -> Result<(JsonValue, Vec<Message>), ExecutionError> {
        log::info!("Starting agent execution: {}", self.agent.name);

        let prompt = self.orchestrator.extract_prompt(self.agent, self.runtime_context)?;
        log::debug!("Agent '{}' prompt: {}", self.agent.name, prompt);

        let schema = self.orchestrator.extract_schema(self.agent)?;
        let done_tool = Arc::new(DoneTool::new(schema.clone()));

        let context = self.build_initial_context(initial_context, &prompt);
        let tools = self.build_tools(done_tool.clone())?;

        self.execute_loop(context, tools, done_tool, schema).await
    }

    fn build_initial_context(&self, mut initial_context: Vec<Message>, prompt: &str) -> Vec<Message> {
        initial_context.push(system! {
            You are an AI agent executing a task. CRITICAL: You have access to a done tool that you MUST use to return your final result.

            IMPORTANT INSTRUCTIONS:
            1. Do NOT respond with plain text - you MUST call the done tool
            2. The done tool is how you return your result to the system
            3. After completing your task, immediately call the done tool with your output
            4. Your response will NOT be processed unless you call the done tool
            5. Think of the done tool as your ONLY way to communicate your final answer

            Example: If asked to generate a number, call done with output 42 and status success instead of just saying 42.

            Remember: ALWAYS call the done tool when you have your answer ready.
        });

        initial_context.push(Message::User {
            content: prompt.to_string(),
        });

        initial_context
    }

    fn build_tools(&self, done_tool: Arc<DoneTool>) -> Result<Vec<ToolDefinition>, ExecutionError> {
        let allowed_tools = self.orchestrator.extract_tools(self.agent, self.runtime_context)?;
        let tools = self
            .orchestrator
            .build_tool_definitions_with_done(done_tool, allowed_tools.as_deref());

        log::info!("Agent '{}' has {} tools available", self.agent.name, tools.len());
        if !tools.is_empty() {
            log::debug!(
                "Agent '{}' tools: {:?}",
                self.agent.name,
                tools.iter().map(|t| &t.name).collect::<Vec<_>>()
            );
        }

        Ok(tools)
    }

    async fn execute_loop(
        &self,
        mut context: Vec<Message>,
        tools: Vec<ToolDefinition>,
        done_tool: Arc<DoneTool>,
        schema: Option<JsonValue>,
    ) -> Result<(JsonValue, Vec<Message>), ExecutionError> {
        let mut iteration_count = 0;

        loop {
            iteration_count += 1;
            log::debug!("Agent '{}' iteration {}", self.agent.name, iteration_count);

            if iteration_count > MAX_ITERATIONS {
                return self.max_iterations_error();
            }

            context = self.execute_iteration(context, &tools).await?;

            if let Some(result) = self.check_for_done(&context, &done_tool, schema.as_ref()).await? {
                return Ok(result);
            }
        }
    }

    async fn execute_iteration(
        &self,
        context: Vec<Message>,
        tools: &[ToolDefinition],
    ) -> Result<Vec<Message>, ExecutionError> {
        log::debug!("Agent '{}' calling provider", self.agent.name);

        let output = self
            .orchestrator
            .provider()
            .execute_agent(self.agent, context, tools.to_vec())
            .await
            .map_err(|error| ExecutionError::ProviderError {
                agent: self.agent.name.clone(),
                message: error.to_string(),
                suggestion: Some("Check provider configuration and connectivity".to_string()),
            })?;

        log::debug!("Agent '{}' received response from provider", self.agent.name);
        Ok(output.context)
    }

    async fn check_for_done(
        &self,
        context: &[Message],
        done_tool: &Arc<DoneTool>,
        schema: Option<&JsonValue>,
    ) -> Result<Option<(JsonValue, Vec<Message>)>, ExecutionError> {
        let tool_calls = context
            .last()
            .and_then(|msg| {
                if let Message::Assistant { tool_calls, .. } = msg {
                    tool_calls.as_ref()
                } else {
                    None
                }
            })
            .cloned();

        if let Some(tool_calls) = tool_calls {
            return self
                .process_tool_calls(context.to_vec(), &tool_calls, done_tool, schema)
                .await;
        }

        log::trace!("Agent '{}' response had no tool calls", self.agent.name);
        Ok(None)
    }

    async fn process_tool_calls(
        &self,
        mut context: Vec<Message>,
        tool_calls: &[crate::providers::provider::ToolCall],
        done_tool: &Arc<DoneTool>,
        schema: Option<&JsonValue>,
    ) -> Result<Option<(JsonValue, Vec<Message>)>, ExecutionError> {
        let mut done_called = false;
        let mut done_output = None;

        log::debug!("Agent '{}' made {} tool calls", self.agent.name, tool_calls.len());

        for tool_call in tool_calls {
            log::info!(
                "Agent '{}' calling tool: {} (id: {})",
                self.agent.name,
                tool_call.name,
                tool_call.id
            );
            log::debug!("Tool '{}' arguments: {}", tool_call.name, tool_call.arguments);

            let tool_result = match self
                .orchestrator
                .execute_tool(&tool_call.name, &tool_call.arguments, Some(done_tool.clone()))
                .await
            {
                Ok(result) => {
                    log::debug!("Tool '{}' executed successfully", tool_call.name);
                    log::trace!("Tool '{}' result: {}", tool_call.name, result);
                    result
                }
                Err(error) => {
                    log::warn!("Tool '{}' execution failed: {}", tool_call.name, error);
                    let error_message =
                        format!("Tool execution error: {error}. Please fix the tool call and try again.");

                    context.push(Message::Tool {
                        tool_call_id: tool_call.id.clone(),
                        content: error_message,
                    });

                    continue;
                }
            };

            if tool_call.name == "done" {
                if let Some(output) = self.handle_done_tool(tool_call, &mut context, schema)? {
                    done_called = true;
                    done_output = Some(output);
                }
            } else {
                context.push(Message::Tool {
                    tool_call_id: tool_call.id.clone(),
                    content: tool_result,
                });
            }
        }

        if done_called {
            if let Some(final_output) = done_output {
                log::info!("Agent '{}' completed successfully with valid output", self.agent.name);
                log::debug!("Agent '{}' final output: {:?}", self.agent.name, final_output);
                return Ok(Some((final_output, context)));
            }
            log::debug!(
                "Agent '{}' done called but output validation failed, continuing loop",
                self.agent.name
            );
        }

        Ok(None)
    }

    fn handle_done_tool(
        &self,
        tool_call: &crate::providers::provider::ToolCall,
        context: &mut Vec<Message>,
        schema: Option<&JsonValue>,
    ) -> Result<Option<JsonValue>, ExecutionError> {
        log::info!("Agent '{}' called done tool", self.agent.name);

        let done_params: serde_json::Map<String, JsonValue> = match serde_json::from_str(&tool_call.arguments) {
            Ok(params) => params,
            Err(error) => {
                log::warn!(
                    "Agent '{}' done tool parameters parse failed: {}",
                    self.agent.name,
                    error
                );
                let error_message = format!(
                        "Failed to parse done tool parameters: {error}. Please ensure you provide 'status' and 'output' fields."
                    );

                context.push(Message::Tool {
                    tool_call_id: tool_call.id.clone(),
                    content: error_message,
                });

                return Ok(None);
            }
        };

        let status: DoneStatus = done_params
            .get("status")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(DoneStatus::Success);
        log::debug!("Agent '{}' done status: {:?}", self.agent.name, status);

        let output_value = done_params.get("output").cloned().unwrap_or(JsonValue::Null);

        if matches!(status, DoneStatus::Fail) {
            log::error!(
                "Agent '{}' failed: {}",
                self.agent.name,
                output_value.as_str().unwrap_or("Unknown error")
            );
            return Err(ExecutionError::RuntimeError {
                agent: self.agent.name.clone(),
                message: format!("Agent failed: {}", output_value.as_str().unwrap_or("Unknown error")),
                suggestion: None,
            });
        }

        if let Some(ref schema_value) = schema {
            log::debug!("Agent '{}' validating output against schema", self.agent.name);
            match SchemaValidator::validate(schema_value, &output_value) {
                Ok(()) => {
                    log::info!("Agent '{}' output validated successfully", self.agent.name);
                    Ok(Some(output_value))
                }
                Err(validation_error) => {
                    log::warn!(
                        "Agent '{}' schema validation failed: {}",
                        self.agent.name,
                        validation_error
                    );
                    let error_message = format!(
                        "Schema validation failed: {validation_error}. Please fix the output and call done again."
                    );

                    context.push(Message::Tool {
                        tool_call_id: tool_call.id.clone(),
                        content: error_message,
                    });

                    Ok(None)
                }
            }
        } else {
            Ok(Some(output_value))
        }
    }

    fn max_iterations_error(&self) -> Result<(JsonValue, Vec<Message>), ExecutionError> {
        log::error!(
            "Agent '{}' exceeded maximum iterations ({})",
            self.agent.name,
            MAX_ITERATIONS
        );
        Err(ExecutionError::RuntimeError {
            agent: self.agent.name.clone(),
            message: format!("Agent exceeded maximum iterations ({MAX_ITERATIONS}). Agent may be stuck in a loop."),
            suggestion: Some("Check agent logic and ensure it calls the done tool to exit".to_string()),
        })
    }
}
