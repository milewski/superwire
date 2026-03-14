use crate::ast::{Agent, AgentProperty, NamedSchema, SchemaReference, Value};
use crate::execution::agent_executor::AgentExecutor;
use crate::execution::context::RuntimeContext;
use crate::execution::error::ExecutionError;
use crate::providers::provider::{Message, ProviderRef, ToolDefinition};
use crate::schemas::SchemaCompiler;
use crate::tools::{Tool, ToolRegistry};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::sync::Arc;

pub struct AgentOrchestrator {
    provider: ProviderRef,
    tool_registry: ToolRegistry,
    schemas: Vec<NamedSchema>,
}

impl AgentOrchestrator {
    pub fn new(provider: ProviderRef) -> Self {
        Self {
            provider,
            tool_registry: ToolRegistry::new(),
            schemas: Vec::new(),
        }
    }

    pub fn with_tools(provider: ProviderRef, tool_registry: ToolRegistry) -> Self {
        Self {
            provider,
            tool_registry,
            schemas: Vec::new(),
        }
    }

    pub fn with_schemas(provider: ProviderRef, tool_registry: ToolRegistry, schemas: Vec<NamedSchema>) -> Self {
        Self {
            provider,
            tool_registry,
            schemas,
        }
    }

    pub async fn execute_agent(
        &self,
        agent: &Agent,
        initial_context: Vec<Message>,
        runtime_context: &RuntimeContext,
    ) -> Result<(JsonValue, Vec<Message>), ExecutionError> {
        let executor = AgentExecutor::new(self, agent, runtime_context);
        executor.execute(initial_context).await
    }

    #[must_use]
    pub fn provider(&self) -> &ProviderRef {
        &self.provider
    }

    pub async fn execute_tool(
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

    pub fn extract_prompt(&self, agent: &Agent, runtime_context: &RuntimeContext) -> Result<String, ExecutionError> {
        for property in &agent.properties {
            if let AgentProperty::Prompt { value, .. } = property {
                return self.value_to_string(value, runtime_context);
            }
        }

        Ok(String::new())
    }

    pub fn extract_tools(
        &self,
        agent: &Agent,
        runtime_context: &RuntimeContext,
    ) -> Result<Option<Vec<String>>, ExecutionError> {
        for property in &agent.properties {
            if let AgentProperty::Tools { value, .. } = property {
                let resolved = runtime_context.resolve_value(value)?;

                if let JsonValue::Array(tools) = resolved {
                    let tool_names: Result<Vec<String>, ExecutionError> = tools
                        .iter()
                        .map(|tool_value| {
                            if let JsonValue::String(tool_ref) = tool_value {
                                if let Some(tool_name) = tool_ref.strip_prefix("tool.") {
                                    Ok(tool_name.to_string())
                                } else {
                                    Err(ExecutionError::RuntimeError {
                                        agent: agent.name.clone(),
                                        message: format!(
                                            "Invalid tool reference: {tool_ref}. Expected format: tool.name"
                                        ),
                                        suggestion: Some("Use format like tool.calculator".to_string()),
                                    })
                                }
                            } else {
                                Err(ExecutionError::RuntimeError {
                                    agent: agent.name.clone(),
                                    message: "Tool reference must be a string".to_string(),
                                    suggestion: Some("Use format like tool.calculator".to_string()),
                                })
                            }
                        })
                        .collect();

                    return Ok(Some(tool_names?));
                }

                return Err(ExecutionError::RuntimeError {
                    agent: agent.name.clone(),
                    message: "Tools property must be an array".to_string(),
                    suggestion: Some("Use format: tools <- [tool.calculator]".to_string()),
                });
            }
        }

        Ok(None)
    }

    pub fn extract_schema(&self, agent: &Agent) -> Result<Option<JsonValue>, ExecutionError> {
        log::debug!(
            "Extracting schema for agent '{}', available schemas: {:?}",
            agent.name,
            self.schemas.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        for property in &agent.properties {
            if let AgentProperty::Output { value, .. } = property {
                match value {
                    SchemaReference::Named(_name) => {
                        let schema_name = _name.strip_prefix("schema.").unwrap_or(_name);

                        let named_schema = self.schemas.iter().find(|s| s.name == schema_name).ok_or_else(|| {
                            ExecutionError::RuntimeError {
                                agent: agent.name.clone(),
                                message: format!("Schema '{schema_name}' not found"),
                                suggestion: Some("Check that the schema is defined in the workflow".to_string()),
                            }
                        })?;

                        let compiled = SchemaCompiler::compile(&named_schema.schema).map_err(|error| {
                            ExecutionError::RuntimeError {
                                agent: agent.name.clone(),
                                message: format!("Failed to compile schema '{schema_name}': {error}"),
                                suggestion: Some("Check schema definition".to_string()),
                            }
                        })?;

                        return Ok(Some(compiled));
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

    fn build_tool_definitions(&self, allowed_tools: Option<&[String]>) -> Vec<ToolDefinition> {
        let all_tools = self.tool_registry.list();

        let filtered_tools = if let Some(allowed) = allowed_tools {
            all_tools
                .into_iter()
                .filter(|tool| allowed.contains(&tool.name().to_string()))
                .collect::<Vec<_>>()
        } else {
            all_tools
        };

        filtered_tools
            .iter()
            .map(|tool| ToolDefinition {
                name: Cow::Owned(tool.name().to_string()),
                description: Cow::Owned(tool.description().to_string()),
                parameters_schema: tool.parameters_schema(),
            })
            .collect()
    }

    pub fn build_tool_definitions_with_done(
        &self,
        done_tool: Arc<dyn Tool>,
        allowed_tools: Option<&[String]>,
    ) -> Vec<ToolDefinition> {
        let mut tools = self.build_tool_definitions(allowed_tools);

        tools.push(ToolDefinition {
            name: Cow::Owned(done_tool.name().to_string()),
            description: Cow::Owned(done_tool.description().to_string()),
            parameters_schema: done_tool.parameters_schema(),
        });

        tools
    }
}
