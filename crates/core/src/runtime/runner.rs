use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::provider::ProviderConfig;
use async_trait::async_trait;
use engine_ai_agent::tool::{registered_runtime_tools, RuntimeTool, ToolError};
use engine_ai_agent::{Agent, AgentConfig, DynamicTool, LoopExecutor, OllamaProvider, OpenAIProvider, Provider};
use schemars::Schema;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RequestedAgentTool {
    pub name: String,
    pub bound_arguments: Map<String, Value>,
}

impl RequestedAgentTool {
    fn merge_with_model_arguments(&self, model_arguments: Value) -> Result<Value, ToolError> {
        let Some(model_argument_fields) = model_arguments.as_object() else {
            return Err(ToolError::new(format!(
                "tool `{}` requires object arguments, but model sent {}",
                self.name,
                crate::runtime::types::value_kind_name(&model_arguments)
            )));
        };

        let mut merged_arguments = model_argument_fields.clone();

        for (bound_argument_name, bound_argument_value) in &self.bound_arguments {
            merged_arguments.insert(bound_argument_name.clone(), bound_argument_value.clone());
        }

        Ok(Value::Object(merged_arguments))
    }
}

#[derive(Clone)]
struct BoundRuntimeTool {
    inner_tool: Arc<dyn RuntimeTool>,
    requested_tool: RequestedAgentTool,
}

impl BoundRuntimeTool {
    fn new(inner_tool: Arc<dyn RuntimeTool>, requested_tool: RequestedAgentTool) -> Self {
        Self {
            inner_tool,
            requested_tool,
        }
    }
}

impl Debug for BoundRuntimeTool {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundRuntimeTool")
            .field("name", &self.requested_tool.name)
            .field("bound_arguments", &self.requested_tool.bound_arguments)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl RuntimeTool for BoundRuntimeTool {
    fn definition(&self) -> Result<engine_ai_agent::ToolDefinition, ToolError> {
        self.inner_tool.definition()
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        // Merge order is deterministic: model-provided arguments are applied first,
        // then DSL-bound arguments override matching keys.
        let merged_input = self.requested_tool.merge_with_model_arguments(input)?;

        self.inner_tool.execute(merged_input).await
    }
}

#[derive(Debug, Clone)]
pub struct AgentExecutionRequest {
    pub agent_name: String,
    pub provider_config: ProviderConfig,
    pub model_name: String,
    pub prompt: String,
    pub config: AgentConfig,
    pub output_schema: Schema,
    pub requested_tools: Vec<RequestedAgentTool>,
    pub runtime_tools: Vec<DynamicTool>,
}

#[derive(Debug, Clone)]
pub struct AgentExecutionResult {
    pub output: Value,
    pub context: Value,
}

#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError>;
}

#[derive(Debug, Clone, Default)]
pub struct LoopAgentRunner;

#[async_trait]
impl AgentRunner for LoopAgentRunner {
    async fn run_agent(&self, request: &AgentExecutionRequest) -> Result<AgentExecutionResult, WorkflowRuntimeError> {
        match &request.provider_config {
            ProviderConfig::OpenAI(openai_provider_config) => {
                let openai_provider = OpenAIProvider::new_with_base_url(
                    openai_provider_config.endpoint.clone(),
                    openai_provider_config.api_key.clone(),
                    request.model_name.clone(),
                );

                self.run_with_provider(openai_provider, request).await
            }
            ProviderConfig::Ollama(ollama_provider_config) => {
                let ollama_provider = OllamaProvider::new(
                    ollama_provider_config.host.clone(),
                    ollama_provider_config.port,
                    request.model_name.clone(),
                );

                self.run_with_provider(ollama_provider, request).await
            }
        }
    }
}

impl LoopAgentRunner {
    async fn run_with_provider<ProviderType>(
        &self,
        provider: ProviderType,
        request: &AgentExecutionRequest,
    ) -> Result<AgentExecutionResult, WorkflowRuntimeError>
    where
        ProviderType: Provider + Send + Sync,
    {
        let resolved_runtime_tools = self.resolve_runtime_tools(request)?;

        let executor = LoopExecutor::<ProviderType, Value>::new()
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to create loop executor for `{}`: {error}", request.agent_name),
            })?
            .with_finalize_answer_schema(request.output_schema.clone())
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to configure finalize schema for agent `{}`: {error}", request.agent_name),
            })?;

        let mut agent = Agent::new_without_registered_tools(executor, provider).with_config(request.config.clone());

        for runtime_tool in resolved_runtime_tools {
            agent = agent.with_runtime_tool(runtime_tool);
        }

        let execution_result = agent
            .run(request.prompt.clone())
            .await
            .map_err(|source| WorkflowRuntimeError::AgentExecutionFailed {
                agent_name: request.agent_name.clone(),
                source: Box::new(source),
            })?;

        let serialized_context =
            serde_json::to_value(execution_result.context).map_err(|source| WorkflowRuntimeError::SerializationFailed {
                context: format!("context for agent `{}`", request.agent_name),
                source,
            })?;

        Ok(AgentExecutionResult {
            output: execution_result.output,
            context: serialized_context,
        })
    }

    fn resolve_runtime_tools(&self, request: &AgentExecutionRequest) -> Result<Vec<Arc<dyn RuntimeTool>>, WorkflowRuntimeError> {
        if request.requested_tools.is_empty() {
            return Ok(Vec::new());
        }

        let mut available_runtime_tools = HashMap::<String, Arc<dyn RuntimeTool>>::new();

        for registered_tool in registered_runtime_tools() {
            let tool_definition = registered_tool.definition().map_err(|error| WorkflowRuntimeError::Other {
                message: format!(
                    "failed to read definition for registered runtime tool while preparing agent `{}` tools: {error}",
                    request.agent_name
                ),
            })?;

            if available_runtime_tools
                .insert(tool_definition.name.clone(), Arc::clone(&registered_tool))
                .is_some()
            {
                return Err(WorkflowRuntimeError::Other {
                    message: format!(
                        "duplicate runtime tool name `{}` while preparing tools for agent `{}`",
                        tool_definition.name, request.agent_name
                    ),
                });
            }
        }

        for dynamic_tool in &request.runtime_tools {
            let dynamic_runtime_tool: Arc<dyn RuntimeTool> = Arc::new(dynamic_tool.clone());
            let tool_definition = dynamic_runtime_tool.definition().map_err(|error| WorkflowRuntimeError::Other {
                message: format!(
                    "failed to read definition for runtime dynamic tool while preparing agent `{}` tools: {error}",
                    request.agent_name
                ),
            })?;

            if available_runtime_tools
                .insert(tool_definition.name.clone(), Arc::clone(&dynamic_runtime_tool))
                .is_some()
            {
                return Err(WorkflowRuntimeError::Other {
                    message: format!(
                        "duplicate runtime tool name `{}` while preparing tools for agent `{}`",
                        tool_definition.name, request.agent_name
                    ),
                });
            }
        }

        let mut resolved_tools = Vec::<Arc<dyn RuntimeTool>>::new();

        for requested_tool in &request.requested_tools {
            let Some(resolved_tool) = available_runtime_tools.get(&requested_tool.name) else {
                return Err(WorkflowRuntimeError::InvalidAgentProperty {
                    agent_name: request.agent_name.clone(),
                    property: "tools".to_string(),
                    message: format!("requested tool `tool.{}` is not available at runtime", requested_tool.name),
                });
            };

            if requested_tool.bound_arguments.is_empty() {
                resolved_tools.push(Arc::clone(resolved_tool));

                continue;
            }

            resolved_tools.push(Arc::new(BoundRuntimeTool::new(Arc::clone(resolved_tool), requested_tool.clone())));
        }

        Ok(resolved_tools)
    }
}
