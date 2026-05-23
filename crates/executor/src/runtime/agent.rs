use super::{AgentExecutionContext, CompletedAgentExecution, ExecutorError, ToolCallExecutionContext, WorkflowExecutor};
use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, ModelRequest, ModelSchema, ModelSchemaCache, ModelToolDefinition, ToolCallTracker};
use crate::runtime::cache::{hash_serializable_value, AgentCacheKey, CachedAgentExecution};
use crate::runtime::mcp::normalize_prompt;
use crate::runtime::schema::{AgentOutputInjections, PlannedAgentSchemaExt};
use crate::runtime::state::RuntimeState;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;
use superwire_core::dsl::{AgentExpressionPropertyName, AgentProperty};
use superwire_core::mcp::McpClientPool;
use superwire_core::semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_core::semantic::support::provider::ProviderConfig;
use superwire_core::semantic::{PlannedAgent, WorkflowSemanticError};
use tokio::sync::mpsc;

pub(in crate::runtime) struct AgentRunContext<'a, ModelProviderType> {
    pub(in crate::runtime) planned_agent: &'a PlannedAgent,
    pub(in crate::runtime) runtime_state: &'a RuntimeState,
    pub(in crate::runtime) model_provider: &'a ModelProviderType,
    pub(in crate::runtime) agent_execution_context: &'a AgentExecutionContext,
    pub(in crate::runtime) iteration_index: Option<usize>,
}

struct PreparedAgentRequest {
    agent_name: String,
    provider_config: ProviderConfig,
    model_name: String,
    inference: HashMap<String, Value>,
    prompt: String,
    output_schema: ModelSchema,
    output_injections: AgentOutputInjections,
    tool_definitions: Vec<ModelToolDefinition>,
    tool_names: Vec<String>,
}

impl PreparedAgentRequest {
    fn cache_key(&self, agent_execution_context: &AgentExecutionContext) -> Result<Option<AgentCacheKey>, ExecutorError> {
        if !agent_execution_context.cache_options.is_enabled() {
            return Ok(None);
        }

        let fingerprint = self.cache_fingerprint()?;
        let agent_hash = hash_serializable_value(&fingerprint)?;

        Ok(Some(AgentCacheKey::new(&agent_execution_context.cache_options.session, agent_hash)))
    }

    fn cache_fingerprint(&self) -> Result<PreparedAgentRequestCacheFingerprint, ExecutorError> {
        let mut schema_cache = ModelSchemaCache::new();
        let provider = ProviderConfigCacheFingerprint {
            driver: self.provider_config.driver.as_str().to_string(),
            endpoint: self.provider_config.endpoint.clone(),
            api_key: self.provider_config.api_key.clone(),
        };
        let inference = self
            .inference
            .iter()
            .map(|(field_name, field_value)| (field_name.clone(), field_value.clone()))
            .collect::<BTreeMap<_, _>>();
        let output_schema = self.output_schema.cache_fingerprint_value(&mut schema_cache);
        let output_injections = self.output_injections.fingerprint_value();
        let tools = self
            .tool_definitions
            .iter()
            .map(|tool_definition| tool_definition.cache_fingerprint_value(&mut schema_cache))
            .collect::<Vec<_>>();

        Ok(PreparedAgentRequestCacheFingerprint {
            version: 1,
            agent_name: self.agent_name.clone(),
            provider,
            model_name: self.model_name.clone(),
            inference,
            prompt: self.prompt.clone(),
            output_schema,
            output_injections,
            tools,
        })
    }

    fn into_model_request(
        self,
        event_sender: Option<mpsc::Sender<ExecutorEvent>>,
        mcp_pool: McpClientPool,
        tool_call_tracker: ToolCallTracker,
    ) -> ModelRequest {
        ModelRequest {
            agent_name: self.agent_name,
            provider_config: self.provider_config,
            model_name: self.model_name,
            inference: self.inference,
            prompt: self.prompt,
            output_schema: self.output_schema,
            tools: self.tool_definitions,
            event_sender,
            mcp_pool,
            tool_call_tracker,
        }
    }
}

#[derive(Debug, Serialize)]
struct PreparedAgentRequestCacheFingerprint {
    version: u8,
    agent_name: String,
    provider: ProviderConfigCacheFingerprint,
    model_name: String,
    inference: BTreeMap<String, Value>,
    prompt: String,
    output_schema: Value,
    output_injections: Value,
    tools: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct ProviderConfigCacheFingerprint {
    driver: String,
    endpoint: Option<String>,
    api_key: Option<String>,
}

impl WorkflowExecutor {
    pub(in crate::runtime) async fn execute_agent<ModelProviderType>(
        &self,
        agent_run_context: AgentRunContext<'_, ModelProviderType>,
    ) -> Result<CompletedAgentExecution, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let planned_agent = agent_run_context.planned_agent;
        let runtime_state = agent_run_context.runtime_state;
        let model_provider = agent_run_context.model_provider;
        let agent_execution_context = agent_run_context.agent_execution_context;
        let agent_started_at = Instant::now();
        let agent_dynamic_values = self.execute_agent_dynamic_blocks(
            planned_agent,
            runtime_state,
            agent_execution_context.event_sender.as_ref(),
            &agent_execution_context.tool_call_tracker,
        )?;

        let evaluation_context = runtime_state.evaluation_context_with_bindings(&agent_dynamic_values);

        log::info!("starting agent `{}`", planned_agent.name);

        let prepared_request = self.prepare_agent_request(planned_agent, &evaluation_context, agent_execution_context)?;
        let mut schema_cache = ModelSchemaCache::new();
        let response_schema_type_name = prepared_request
            .output_schema
            .schema_type_name_with_cache(&mut schema_cache)
            .unwrap_or_else(|| "unknown".to_string());

        log::debug!(
            "agent `{}` request prepared: model={}, tools={}, response_schema={}",
            planned_agent.name,
            prepared_request.model_name,
            prepared_request.tool_definitions.len(),
            response_schema_type_name
        );

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_started(
                    planned_agent.name.clone(),
                    prepared_request.model_name.clone(),
                    prepared_request.tool_names.clone(),
                    agent_run_context.iteration_index,
                ))
                .await;
        }

        let cache_key = prepared_request.cache_key(agent_execution_context)?;

        if let Some(cache_key) = cache_key.as_ref() {
            if let Some(cache_store) = &agent_execution_context.cache_options.store {
                if let Some(cached_execution) = cache_store.get(cache_key)? {
                    log::debug!("agent `{}` cache hit: hash={}", planned_agent.name, cache_key.agent_hash());

                    planned_agent.validate_output_value(&cached_execution.output)?;

                    if let Some(event_sender) = &agent_execution_context.event_sender {
                        let _ = event_sender
                            .send(ExecutorEvent::agent_completed(
                                planned_agent.name.clone(),
                                cached_execution.output.clone(),
                                agent_started_at.elapsed(),
                                agent_run_context.iteration_index,
                                true,
                            ))
                            .await;
                    }

                    return Ok(CompletedAgentExecution {
                        agent_name: planned_agent.name.clone(),
                        output: cached_execution.output,
                        context: cached_execution.context,
                    });
                }
            }
        }

        let output_injections = prepared_request.output_injections.clone();
        let mut model_response = model_provider
            .generate(prepared_request.into_model_request(
                agent_execution_context.event_sender.clone(),
                self.mcp_pool.clone(),
                agent_execution_context.tool_call_tracker.clone(),
            ))
            .await?;

        log::debug!("agent `{}` model response received", planned_agent.name);

        model_response.output = planned_agent.inject_output_values(model_response.output, &output_injections)?;

        planned_agent.validate_output_value(&model_response.output)?;

        if let Some(cache_key) = cache_key {
            if let Some(cache_store) = &agent_execution_context.cache_options.store {
                cache_store.put(
                    cache_key,
                    CachedAgentExecution::new(model_response.output.clone(), model_response.context.clone()),
                    agent_execution_context.cache_options.time_to_live,
                )?;
            }
        }

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_completed(
                    planned_agent.name.clone(),
                    model_response.output.clone(),
                    agent_started_at.elapsed(),
                    agent_run_context.iteration_index,
                    false,
                ))
                .await;
        }

        Ok(CompletedAgentExecution {
            agent_name: planned_agent.name.clone(),
            output: model_response.output,
            context: model_response.context,
        })
    }

    fn prepare_agent_request(
        &self,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
        agent_execution_context: &AgentExecutionContext,
    ) -> Result<PreparedAgentRequest, ExecutorError> {
        let provider_template = self
            .execution_plan
            .provider_index
            .get(&planned_agent.provider_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("provider `{}` is not declared", planned_agent.provider_name),
            })?;

        let provider_config = provider_template.resolve(&planned_agent.provider_name, evaluation_context)?;
        let model_name = planned_agent.evaluate_model_name(evaluation_context)?;
        let inference = self.evaluate_inference_fields(planned_agent, evaluation_context)?;
        let instruction_expression = planned_agent
            .declaration
            .required_expression_property(AgentExpressionPropertyName::Instruction)
            .map_err(|missing_property| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: planned_agent.name.clone(),
                property: missing_property.as_str().to_string(),
                message: "property is required".to_string(),
            })?;

        let tool_call_execution_context =
            ToolCallExecutionContext::new(evaluation_context, None, &agent_execution_context.tool_call_tracker);
        let agent_instruction_value = self.evaluate_runtime_expression(
            instruction_expression,
            tool_call_execution_context,
            &format!("instruction for agent `{}`", planned_agent.name),
        )?;
        let agent_instruction = normalize_prompt(&agent_instruction_value);
        let prompt = if agent_execution_context.import_context.is_empty() {
            agent_instruction
        } else {
            format!("{}\n\n{agent_instruction}", agent_execution_context.import_context)
        };
        let mut tool_definitions = self.resolve_agent_use_definitions(planned_agent, evaluation_context)?;
        let output_schema = planned_agent.push_finalize_tool_definition(&mut tool_definitions);
        let output_injections = planned_agent.output_injections(evaluation_context)?;
        let tool_names = tool_definitions
            .iter()
            .map(ModelToolDefinition::event_display_name)
            .collect::<Vec<_>>();

        Ok(PreparedAgentRequest {
            agent_name: planned_agent.name.clone(),
            provider_config,
            model_name,
            inference,
            prompt,
            output_schema,
            output_injections,
            tool_definitions,
            tool_names,
        })
    }

    fn execute_agent_dynamic_blocks(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &RuntimeState,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<HashMap<String, Value>, ExecutorError> {
        let mut dynamic_values = HashMap::new();

        for agent_property in &planned_agent.declaration.properties {
            let AgentProperty::Dynamic(dynamic_block) = agent_property else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                let evaluation_context = runtime_state.evaluation_context_with_bindings(&dynamic_values);
                let tool_call_execution_context = ToolCallExecutionContext::new(&evaluation_context, event_sender, tool_call_tracker);
                let field_value = self.evaluate_runtime_expression(
                    &dynamic_field.value,
                    tool_call_execution_context,
                    &format!("dynamic field `{}` for agent `{}`", dynamic_field.name, planned_agent.name),
                )?;
                dynamic_values.insert(dynamic_field.name.clone(), field_value);
            }
        }

        Ok(dynamic_values)
    }

    fn evaluate_inference_fields(
        &self,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
    ) -> Result<HashMap<String, Value>, ExecutorError> {
        let mut inference = HashMap::new();

        for inference_field in &planned_agent.inference_fields {
            let context = format!("inference setting `{}` for agent `{}`", inference_field.name, planned_agent.name);
            let value = evaluate_expression(&inference_field.value, evaluation_context, &context)?;
            inference.insert(inference_field.name.clone(), value);
        }

        Ok(inference)
    }
}

trait PlannedAgentRuntimeExt {
    fn evaluate_model_name(&self, evaluation_context: &EvaluationContext) -> Result<String, ExecutorError>;
}

impl PlannedAgentRuntimeExt for PlannedAgent {
    fn evaluate_model_name(&self, evaluation_context: &EvaluationContext) -> Result<String, ExecutorError> {
        let model_value = evaluate_expression(
            &self.model_id_expression,
            evaluation_context,
            &format!("model for agent `{}`", self.name),
        )?;

        model_value.as_str().map(str::to_string).ok_or_else(|| ExecutorError::Other {
            message: format!("model for agent `{}` must resolve to string", self.name),
        })
    }
}
