use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::provider::ProviderConfig;
use async_trait::async_trait;
use engine_ai_agent::{Agent, AgentConfig, LoopExecutor, OllamaProvider, OpenAIProvider, Provider};
use schemars::Schema;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AgentExecutionRequest {
    pub agent_name: String,
    pub provider_config: ProviderConfig,
    pub model_name: String,
    pub prompt: String,
    pub config: AgentConfig,
    pub output_schema: Schema,
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
        let executor = LoopExecutor::<ProviderType, Value>::new()
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to create loop executor for `{}`: {error}", request.agent_name),
            })?
            .with_finalize_answer_schema(request.output_schema.clone())
            .map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to configure finalize schema for agent `{}`: {error}", request.agent_name),
            })?;

        let execution_result = Agent::new(executor, provider)
            .with_config(request.config.clone())
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
}
