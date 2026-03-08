use std::sync::Arc;

use log::{debug, info};
use serde_json::{Map, Value};

use crate::ast::{AgentDefinition, WorkflowDocument};
use crate::execution::error::ExecutionError;
use crate::providers::provider::{ProviderModelConfig, ProviderRequest};
use crate::providers::registry::DynProvider;
use crate::schemas::compiler::validate_value;
use crate::tools::done::DoneTool;
use crate::tools::tool::Tool;

#[derive(Debug, Clone)]
pub struct AgentExecutionResult {
    pub status: String,
    pub output: Value,
    pub transcript: Vec<String>,
    pub context: AgentRuntimeContext,
}

#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    User {
        value: String,
    },
    Assistant {
        value: String,
    },
    System {
        value: String,
    },
    ToolCall {
        name: String,
        arguments: Value,
        result: Value,
    },
}

#[derive(Debug, Clone, Default)]
pub struct AgentRuntimeContext {
    pub messages: Vec<RuntimeMessage>,
    pub summary: Option<String>,
}

impl AgentRuntimeContext {
    pub fn as_json(&self) -> Value {
        Value::Array(self.messages.iter().map(RuntimeMessage::as_json).collect())
    }

    pub fn render_for_prompt(&self) -> String {
        self.messages
            .iter()
            .map(RuntimeMessage::render)
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl RuntimeMessage {
    pub fn as_json(&self) -> Value {
        let mut message_map = Map::new();

        match self {
            RuntimeMessage::User { value } => {
                message_map.insert("type".to_string(), Value::String("user".to_string()));
                message_map.insert("value".to_string(), Value::String(value.clone()));
            }
            RuntimeMessage::Assistant { value } => {
                message_map.insert("type".to_string(), Value::String("assistant".to_string()));
                message_map.insert("value".to_string(), Value::String(value.clone()));
            }
            RuntimeMessage::System { value } => {
                message_map.insert("type".to_string(), Value::String("system".to_string()));
                message_map.insert("value".to_string(), Value::String(value.clone()));
            }
            RuntimeMessage::ToolCall {
                name,
                arguments,
                result,
            } => {
                message_map.insert("type".to_string(), Value::String("tool_call".to_string()));
                message_map.insert("name".to_string(), Value::String(name.clone()));
                message_map.insert("arguments".to_string(), arguments.clone());
                message_map.insert("result".to_string(), result.clone());
            }
        }

        Value::Object(message_map)
    }

    pub fn render(&self) -> String {
        match self {
            RuntimeMessage::User { value } => format!("User: {}", value),
            RuntimeMessage::Assistant { value } => format!("Assistant: {}", value),
            RuntimeMessage::System { value } => format!("System: {}", value),
            RuntimeMessage::ToolCall {
                name,
                arguments,
                result,
            } => format!(
                "Tool Call: {}\nArguments: {}\nResult: {}",
                name,
                serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string()),
                serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
            ),
        }
    }
}

pub async fn execute_agent(
    agent: &AgentDefinition,
    document: &WorkflowDocument,
    provider: DynProvider,
    model: ProviderModelConfig,
) -> Result<AgentExecutionResult, ExecutionError> {
    let done = Arc::new(DoneTool);
    let tools = [done.spec()];
    let prompt = agent.prompt.as_ref().map(render_expression).unwrap_or_default();

    let mut messages = Vec::new();

    if let Some(output_definition) = &agent.output {
        let schema = match output_definition {
            crate::ast::OutputDefinition::Inline(schema) => schema.clone(),
            crate::ast::OutputDefinition::SchemaReference(name) => document
                .schemas
                .iter()
                .find(|schema| schema.name.as_deref() == Some(name))
                .ok_or_else(|| ExecutionError::MissingSchema {
                    agent: agent.name.clone(),
                    schema: name.clone(),
                })?
                .clone(),
        };

        let schema_json =
            crate::schemas::compiler::compile_schema(&schema).map_err(|error| ExecutionError::SchemaCompilation {
                agent: agent.name.clone(),
                message: error.to_string(),
            })?;

        let schema_instruction = format!(
            "You must return your response as JSON following this exact schema:\n\n{}\n\nEnsure your output is valid JSON that matches this structure.",
            serde_json::to_string_pretty(&schema_json).unwrap_or_else(|_| schema_json.to_string())
        );

        debug!(
            "injecting schema as system message: agent={}, schema_size={}",
            agent.name,
            schema_instruction.len()
        );

        messages.push(RuntimeMessage::System {
            value: schema_instruction,
        });
    }

    messages.push(RuntimeMessage::User { value: prompt.clone() });

    info!(
        "starting agent execution: agent={}, provider={}, model={}, tools={}",
        agent.name,
        model.provider_name,
        model.model_name,
        tools.len()
    );

    let mut transcript = Vec::new();
    let max_iterations = 10;

    for iteration in 0..max_iterations {
        let request = ProviderRequest {
            prompt: messages
                .iter()
                .map(RuntimeMessage::render)
                .collect::<Vec<_>>()
                .join("\n\n"),
            tools: tools
                .iter()
                .map(|tool| crate::providers::provider::ToolDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                })
                .collect(),
            response_schema: None,
        };

        let response = provider
            .chat(&model, &request)
            .await
            .map_err(ExecutionError::Provider)?;

        debug!(
            "agent response received: agent={}, iteration={}, message={}",
            agent.name, iteration, response.message
        );

        messages.push(RuntimeMessage::Assistant {
            value: response.message.clone(),
        });
        transcript.push(response.message.clone());

        if let Some(tool_call) = response.tool_calls.iter().find(|call| call.name == "done") {
            let payload = done
                .invoke(tool_call.arguments.clone())
                .await
                .map_err(ExecutionError::Tool)?;
            messages.push(RuntimeMessage::ToolCall {
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
                result: payload.clone(),
            });
            debug!("done tool invoked: agent={}, payload={}", agent.name, payload);

            let status = payload
                .get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| ExecutionError::InvalidDonePayload {
                    agent: agent.name.clone(),
                    message: "missing done.status".into(),
                })?
                .to_owned();

            let mut output = payload
                .get("output")
                .cloned()
                .ok_or_else(|| ExecutionError::InvalidDonePayload {
                    agent: agent.name.clone(),
                    message: "missing done.output".into(),
                })?;

            if let Value::String(ref s) = output {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    output = parsed;
                }
            }

            if status == "success" {
                if let Some(output_def) = &agent.output {
                    let schema = match output_def {
                        crate::ast::OutputDefinition::Inline(schema) => schema.clone(),
                        crate::ast::OutputDefinition::SchemaReference(name) => document
                            .schemas
                            .iter()
                            .find(|s| s.name.as_deref() == Some(name))
                            .ok_or_else(|| ExecutionError::MissingSchema {
                                agent: agent.name.clone(),
                                schema: name.clone(),
                            })?
                            .clone(),
                    };

                    if let Err(validation_error) = validate_value(&schema, &output) {
                        let error_message = format!("Schema validation failed: {}", validation_error);
                        messages.push(RuntimeMessage::System {
                            value: error_message.clone(),
                        });
                        info!(
                            "schema validation failed: agent={}, error={}",
                            agent.name, error_message
                        );
                        continue;
                    }

                    info!("schema validation succeeded: agent={}", agent.name);
                }
            }

            return Ok(AgentExecutionResult {
                status,
                output,
                transcript,
                context: AgentRuntimeContext {
                    messages,
                    summary: None,
                },
            });
        }

        if response.tool_calls.is_empty() {
            messages.push(RuntimeMessage::System {
                value: "You must call the 'done' tool to complete your task. Call done with status='success' and your final output, or status='fail' with an error message.".into(),
            });
        }
    }

    Err(ExecutionError::MissingDoneCall {
        agent: agent.name.clone(),
    })
}

pub async fn summarize_context(
    document: &WorkflowDocument,
    agent_name: &str,
    provider: DynProvider,
    context: &AgentRuntimeContext,
) -> Result<String, ExecutionError> {
    let agent = document
        .agents
        .iter()
        .find(|agent| agent.name == agent_name)
        .ok_or_else(|| ExecutionError::MissingAgentResult {
            agent: agent_name.to_owned(),
        })?;
    let model_ref = agent.model.as_ref().ok_or_else(|| ExecutionError::MissingModel {
        agent: agent_name.to_owned(),
    })?;
    let provider_definition = document
        .providers
        .iter()
        .find(|provider_def| provider_def.name == model_ref.provider)
        .ok_or_else(|| ExecutionError::MissingProviderDefinition {
            provider: model_ref.provider.clone(),
        })?;
    let model = crate::providers::registry::resolve_model_config(provider_definition, &model_ref.model);
    let request = ProviderRequest {
        prompt: format!(
            "Summarize the following agent conversation history. Return only the summary text as plain text, with no JSON, no markdown code fences, and no tool call formatting:\n\n{}",
            context.render_for_prompt()
        ),
        tools: Vec::new(),
        response_schema: None,
    };
    let response = provider.chat(&model, &request).await?;
    Ok(response.message.trim().trim_matches('`').trim().to_string())
}

pub fn render_expression(expression: &crate::ast::Expression) -> String {
    match expression {
        crate::ast::Expression::String(value)
        | crate::ast::Expression::MultilineString(value)
        | crate::ast::Expression::InterpolatedString(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}
