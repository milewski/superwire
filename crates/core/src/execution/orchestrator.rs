use crate::ast::{Agent, AgentProperty, SchemaReference, Value};
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::providers::provider::{Message, ProviderRef, ToolDefinition};
use crate::schemas::{SchemaCompiler, SchemaValidator};
use crate::tools::{DoneStatus, DoneTool, Tool, ToolRegistry};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::sync::Arc;

macro_rules! system {
    ($($content:tt)*) => {{
        let text = stringify!($($content)*);
        // Remove leading/trailing whitespace from each line
        // Join with newlines to preserve paragraph structure
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

pub struct AgentOrchestrator {
    provider: ProviderRef,
    tool_registry: ToolRegistry,
}

impl AgentOrchestrator {
    pub fn new(provider: ProviderRef) -> Self {
        Self {
            provider,
            tool_registry: ToolRegistry::new(),
        }
    }

    pub fn with_tools(provider: ProviderRef, tool_registry: ToolRegistry) -> Self {
        Self {
            provider,
            tool_registry,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn execute_agent(
        &self,
        agent: &Agent,
        initial_context: Vec<Message>,
        runtime_context: &RuntimeContext,
    ) -> Result<(JsonValue, Vec<Message>), ExecutionError> {
        log::info!("Starting agent execution: {}", agent.name);

        let prompt = self.extract_prompt(agent, runtime_context)?;
        log::debug!("Agent '{}' prompt: {}", agent.name, prompt);

        let schema = self.extract_schema(agent)?;

        let done_tool = Arc::new(DoneTool::new(schema.clone()));

        let mut context = Vec::new();

        context.extend(initial_context);

        context.push(system! {
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

        context.push(Message::User {
            content: prompt.clone(),
        });

        let tools = self.build_tool_definitions_with_done(done_tool.clone());

        log::info!("Agent '{}' has {} tools available", agent.name, tools.len());
        if !tools.is_empty() {
            log::debug!(
                "Agent '{}' tools: {:?}",
                agent.name,
                tools.iter().map(|t| &t.name).collect::<Vec<_>>()
            );
        }

        let mut iteration_count = 0;

        loop {
            iteration_count += 1;

            log::debug!("Agent '{}' iteration {}", agent.name, iteration_count);

            if iteration_count > MAX_ITERATIONS {
                log::error!(
                    "Agent '{}' exceeded maximum iterations ({})",
                    agent.name,
                    MAX_ITERATIONS
                );
                return Err(ExecutionError::RuntimeError {
                    agent: agent.name.clone(),
                    message: format!(
                        "Agent exceeded maximum iterations ({MAX_ITERATIONS}). Agent may be stuck in a loop."
                    ),
                    suggestion: Some("Check agent logic and ensure it calls the done tool to exit".to_string()),
                });
            }

            log::debug!("Agent '{}' calling provider", agent.name);

            let output = self
                .provider
                .execute_agent(agent, context.clone(), tools.clone())
                .await
                .map_err(|error| ExecutionError::ProviderError {
                    agent: agent.name.clone(),
                    message: error.to_string(),
                    suggestion: Some("Check provider configuration and connectivity".to_string()),
                })?;

            log::debug!("Agent '{}' received response from provider", agent.name);

            context.clone_from(&output.context);

            if let Some(tool_calls) = context
                .last()
                .and_then(|msg| {
                    if let Message::Assistant { tool_calls, .. } = msg {
                        tool_calls.as_ref()
                    } else {
                        None
                    }
                })
                .cloned()
            {
                let mut done_called = false;
                let mut done_output = None;

                log::debug!("Agent '{}' made {} tool calls", agent.name, tool_calls.len());

                for tool_call in &tool_calls {
                    log::info!(
                        "Agent '{}' calling tool: {} (id: {})",
                        agent.name,
                        tool_call.name,
                        tool_call.id
                    );
                    log::debug!("Tool '{}' arguments: {}", tool_call.name, tool_call.arguments);

                    let tool_result = match self
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
                        log::info!("Agent '{}' called done tool", agent.name);
                        done_called = true;

                        let done_params: serde_json::Map<String, JsonValue> = match serde_json::from_str(
                            &tool_call.arguments,
                        ) {
                            Ok(params) => params,
                            Err(error) => {
                                log::warn!("Agent '{}' done tool parameters parse failed: {}", agent.name, error);
                                let error_message = format!(
                                    "Failed to parse done tool parameters: {error}. Please ensure you provide 'status' and 'output' fields."
                                );

                                context.push(Message::Tool {
                                    tool_call_id: tool_call.id.clone(),
                                    content: error_message,
                                });

                                continue;
                            }
                        };

                        let status: DoneStatus = done_params
                            .get("status")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or(DoneStatus::Success);
                        log::debug!("Agent '{}' done status: {:?}", agent.name, status);

                        let output_value = done_params.get("output").cloned().unwrap_or(JsonValue::Null);

                        if matches!(status, DoneStatus::Fail) {
                            log::error!(
                                "Agent '{}' failed: {}",
                                agent.name,
                                output_value.as_str().unwrap_or("Unknown error")
                            );
                            return Err(ExecutionError::RuntimeError {
                                agent: agent.name.clone(),
                                message: format!("Agent failed: {}", output_value.as_str().unwrap_or("Unknown error")),
                                suggestion: None,
                            });
                        }

                        if let Some(ref schema_value) = schema {
                            log::debug!("Agent '{}' validating output against schema", agent.name);
                            match SchemaValidator::validate(schema_value, &output_value) {
                                Ok(()) => {
                                    log::info!("Agent '{}' output validated successfully", agent.name);
                                    done_output = Some(output_value);
                                }
                                Err(validation_error) => {
                                    log::warn!("Agent '{}' schema validation failed: {}", agent.name, validation_error);
                                    let error_message = format!(
                                        "Schema validation failed: {validation_error}. Please fix the output and call done again."
                                    );

                                    context.push(Message::Tool {
                                        tool_call_id: tool_call.id.clone(),
                                        content: error_message,
                                    });
                                }
                            }
                        } else {
                            done_output = Some(output_value);
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
                        log::info!("Agent '{}' completed successfully with valid output", agent.name);
                        log::debug!("Agent '{}' final output: {:?}", agent.name, final_output);
                        return Ok((final_output, context));
                    }
                    log::debug!(
                        "Agent '{}' done called but output validation failed, continuing loop",
                        agent.name
                    );
                }
            } else {
                log::trace!("Agent '{}' response had no tool calls", agent.name);
            }
        }
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments_json: &str,
        done_tool: Option<Arc<dyn Tool>>,
    ) -> Result<String, ExecutionError> {
        let tool = if tool_name == "done" {
            done_tool.ok_or_else(|| ExecutionError::RuntimeError {
                agent: "tool_execution".to_string(),
                message: "Done tool not provided".to_string(),
                suggestion: Some("This is an internal error".to_string()),
            })?
        } else {
            self.tool_registry
                .get(tool_name)
                .ok_or_else(|| ExecutionError::RuntimeError {
                    agent: "tool_execution".to_string(),
                    message: format!("Unknown tool: {tool_name}"),
                    suggestion: Some("Check that the tool is registered".to_string()),
                })?
        };

        let arguments: JsonValue =
            serde_json::from_str(arguments_json).map_err(|error| ExecutionError::RuntimeError {
                agent: "tool_execution".to_string(),
                message: format!("Failed to parse tool arguments: {error}"),
                suggestion: Some("Ensure tool arguments are valid JSON".to_string()),
            })?;

        let result = tool
            .execute(arguments)
            .await
            .map_err(|error| ExecutionError::RuntimeError {
                agent: "tool_execution".to_string(),
                message: format!("Tool execution failed: {error}"),
                suggestion: None,
            })?;

        Ok(serde_json::to_string(&result).unwrap_or_else(|_| result.to_string()))
    }

    fn extract_prompt(&self, agent: &Agent, runtime_context: &RuntimeContext) -> Result<String, ExecutionError> {
        for property in &agent.properties {
            if let AgentProperty::Prompt { value, .. } = property {
                return self.value_to_string(value, runtime_context);
            }
        }

        Ok(String::new())
    }

    fn extract_schema(&self, agent: &Agent) -> Result<Option<JsonValue>, ExecutionError> {
        for property in &agent.properties {
            if let AgentProperty::Output { value, .. } = property {
                match value {
                    SchemaReference::Named(_name) => {
                        return Ok(None);
                    }
                    SchemaReference::Inline(schema) => {
                        let compiled =
                            SchemaCompiler::compile(schema).map_err(|error| ExecutionError::RuntimeError {
                                agent: agent.name.clone(),
                                message: format!("Failed to compile schema: {error}"),
                                suggestion: Some("Check schema definition".to_string()),
                            })?;

                        return Ok(Some(compiled));
                    }
                    SchemaReference::InlineType {
                        schema_type,
                        description,
                    } => {
                        let compiled =
                            SchemaCompiler::compile_type(schema_type, description.as_deref()).map_err(|error| {
                                ExecutionError::RuntimeError {
                                    agent: agent.name.clone(),
                                    message: format!("Failed to compile schema type: {error}"),
                                    suggestion: Some("Check schema type definition".to_string()),
                                }
                            })?;

                        log::debug!(
                            "Compiled inline type schema: {}",
                            serde_json::to_string_pretty(&compiled).unwrap_or_default()
                        );

                        return Ok(Some(compiled));
                    }
                }
            }
        }

        Ok(None)
    }

    fn value_to_string(&self, value: &Value, runtime_context: &RuntimeContext) -> Result<String, ExecutionError> {
        let resolved = runtime_context.resolve_value(value)?;

        match resolved {
            JsonValue::String(string) => Ok(string),
            other => Ok(other.to_string()),
        }
    }

    fn build_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_registry
            .list()
            .iter()
            .map(|tool| ToolDefinition {
                name: Cow::Owned(tool.name().to_string()),
                description: Cow::Owned(tool.description().to_string()),
                parameters_schema: tool.parameters_schema(),
            })
            .collect()
    }

    fn build_tool_definitions_with_done(&self, done_tool: Arc<dyn Tool>) -> Vec<ToolDefinition> {
        let mut tools = self.build_tool_definitions();

        tools.push(ToolDefinition {
            name: Cow::Owned(done_tool.name().to_string()),
            description: Cow::Owned(done_tool.description().to_string()),
            parameters_schema: done_tool.parameters_schema(),
        });

        tools
    }
}
