use crate::ast::{Agent, AgentProperty, SchemaReference, Value};
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::providers::provider::{Message, ProviderRef, ToolDefinition};
use crate::schemas::{SchemaCompiler, SchemaValidator};
use crate::tools::{DoneTool, ToolRegistry};
use serde_json::Value as JsonValue;
use std::sync::Arc;

pub struct AgentOrchestrator {
    provider: ProviderRef,
    tool_registry: ToolRegistry,
}

impl AgentOrchestrator {
    pub fn new(provider: ProviderRef) -> Self {
        let mut tool_registry = ToolRegistry::new();

        tool_registry.register(Arc::new(DoneTool::new()));

        Self {
            provider,
            tool_registry,
        }
    }

    pub async fn execute_agent(
        &self,
        agent: &Agent,
        initial_context: Vec<Message>,
        runtime_context: &RuntimeContext,
    ) -> Result<(JsonValue, Vec<Message>), ExecutionError> {
        let mut context = initial_context;

        let prompt = self.extract_prompt(agent, runtime_context)?;
        let schema = self.extract_schema(agent)?;

        context.push(Message::User {
            content: prompt.clone(),
        });

        if let Some(ref schema_value) = schema {
            let schema_instruction = SchemaValidator::inject_schema_into_prompt(schema_value);

            context.push(Message::User {
                content: schema_instruction,
            });
        }

        let tools = self.build_tool_definitions();

        let output = self
            .provider
            .execute_agent(agent, context.clone(), tools)
            .await
            .map_err(|error| ExecutionError::ProviderError {
                agent: agent.name.clone(),
                message: error.to_string(),
                suggestion: Some("Check provider configuration and connectivity".to_string()),
            })?;

        let parsed_output = if schema.is_some() {
            if let JsonValue::String(string) = &output.output {
                serde_json::from_str(string).unwrap_or_else(|_| output.output.clone())
            } else {
                output.output.clone()
            }
        } else {
            output.output
        };

        if let Some(ref schema_value) = schema {
            SchemaValidator::validate(schema_value, &parsed_output).map_err(|error| {
                ExecutionError::SchemaValidationError {
                    agent: agent.name.clone(),
                    message: error.to_string(),
                    field_path: None,
                    suggestion: Some("Ensure output matches the defined schema".to_string()),
                }
            })?;
        }

        Ok((parsed_output, output.context))
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
                                message: format!("Failed to compile schema: {}", error),
                                suggestion: Some("Check schema definition".to_string()),
                            })?;

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
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters_schema: tool.parameters_schema(),
            })
            .collect()
    }
}
