use std::sync::Arc;

use log::{debug, info};
use serde_json::Value;

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

#[derive(Debug, Clone, Default)]
pub struct AgentRuntimeContext {
    pub messages: Vec<String>,
    pub summary: Option<String>,
}

pub async fn execute_agent(
    agent: &AgentDefinition,
    document: &WorkflowDocument,
    provider: DynProvider,
    model: ProviderModelConfig,
) -> Result<AgentExecutionResult, ExecutionError> {
    let done = Arc::new(DoneTool::default());
    let tools = vec![done.spec()];
    let prompt = agent
        .prompt
        .as_ref()
        .map(render_expression)
        .unwrap_or_default();

    info!(
        "starting agent execution: agent={}, provider={}, model={}, tools={}",
        agent.name,
        model.provider_name,
        model.model_name,
        tools.len()
    );

    let mut messages = Vec::new();
    let mut transcript = Vec::new();
    let max_iterations = 10;

    for iteration in 0..max_iterations {
        let request = ProviderRequest {
            prompt: if iteration == 0 {
                prompt.clone()
            } else {
                messages.join("\n\n")
            },
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

        messages.push(format!("Assistant: {}", response.message.clone()));
        transcript.push(response.message.clone());

        if let Some(tool_call) = response.tool_calls.iter().find(|call| call.name == "done") {
            let payload = done
                .invoke(tool_call.arguments.clone())
                .await
                .map_err(ExecutionError::Tool)?;
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
                        messages.push(format!("System: {}", error_message));
                        info!("schema validation failed: agent={}, error={}", agent.name, error_message);
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
            messages.push("System: You must call the 'done' tool to complete your task. Call done with status='success' and your final output, or status='fail' with an error message.".into());
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
    let model_ref = agent
        .model
        .as_ref()
        .ok_or_else(|| ExecutionError::MissingModel {
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
            "Summarize the following agent context for reuse by another agent:\n\n{}",
            context.messages.join("\n")
        ),
        tools: Vec::new(),
        response_schema: None,
    };
    let response = provider.chat(&model, &request).await?;
    Ok(response.message)
}

pub fn render_expression(expression: &crate::ast::Expression) -> String {
    match expression {
        crate::ast::Expression::String(value)
        | crate::ast::Expression::MultilineString(value)
        | crate::ast::Expression::InterpolatedString(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}
