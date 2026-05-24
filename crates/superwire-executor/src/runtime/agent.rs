use super::{AgentExecutionContext, CompletedAgentExecution, ExecutorError, ToolCallExecutionContext, WorkflowExecutor};
use crate::model::{
    ModelAsset, ModelPromptContent, ModelProvider, ModelRequest, ModelSchema, ModelSchemaCache, ModelToolDefinition, ToolCallTracker,
};
use crate::runtime::cache::{hash_serializable_value, AgentCacheKey, CachedAgentExecution};
use crate::runtime::mcp::normalize_prompt;
use crate::runtime::schema::{AgentOutputInjections, PlannedAgentSchemaExt};
use crate::runtime::state::RuntimeState;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;
use superwire_dsl::{AgentContext, AgentExpressionPropertyName, AgentProperty, Expression, ModelAssetKind};
use superwire_mcp::McpClientPool;
use superwire_protocol::event::ExecutorEvent;
use superwire_semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_semantic::support::provider::ProviderConfig;
use superwire_semantic::support::types::WorkflowType;
use superwire_semantic::{PlannedAgent, WorkflowSemanticError};
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
    context: Option<Value>,
    prompt: String,
    prompt_content: Vec<ModelPromptContent>,
    supported_asset_kinds: Vec<ModelAssetKind>,
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
            context: self.context.clone(),
            prompt_content: self
                .prompt_content
                .iter()
                .map(ModelPromptContent::fingerprint_value)
                .collect::<Vec<_>>(),
            supported_asset_kinds: self
                .supported_asset_kinds
                .iter()
                .map(|asset_kind| asset_kind.as_str())
                .collect::<Vec<_>>(),
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
            context: self.context,
            prompt: self.prompt,
            prompt_content: self.prompt_content,
            output_schema: self.output_schema,
            tools: self.tool_definitions,
            event_sender,
            mcp_pool,
            tool_call_tracker,
        }
    }

    async fn completed_from_cache(
        &self,
        cache_key: Option<&AgentCacheKey>,
        planned_agent: &PlannedAgent,
        agent_execution_context: &AgentExecutionContext,
        agent_started_at: Instant,
        iteration_index: Option<usize>,
    ) -> Result<Option<CompletedAgentExecution>, ExecutorError> {
        let Some(cache_key) = cache_key else {
            return Ok(None);
        };
        let Some(cache_store) = &agent_execution_context.cache_options.store else {
            return Ok(None);
        };
        let Some(cached_execution) = cache_store.get(cache_key)? else {
            return Ok(None);
        };

        log::debug!("agent `{}` cache hit: hash={}", planned_agent.name, cache_key.agent_hash());

        planned_agent.validate_output_value(&cached_execution.output)?;

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_completed(
                    planned_agent.name.clone(),
                    cached_execution.output.clone(),
                    agent_started_at.elapsed(),
                    iteration_index,
                    true,
                ))
                .await;
        }

        Ok(Some(CompletedAgentExecution {
            agent_name: planned_agent.name.clone(),
            output: cached_execution.output,
            context: cached_execution.context,
        }))
    }
}

#[derive(Debug, Serialize)]
struct PreparedAgentRequestCacheFingerprint {
    version: u8,
    agent_name: String,
    provider: ProviderConfigCacheFingerprint,
    model_name: String,
    inference: BTreeMap<String, Value>,
    context: Option<Value>,
    prompt: String,
    prompt_content: Vec<Value>,
    supported_asset_kinds: Vec<&'static str>,
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

struct AgentContextResolutionRequest<'a, ModelProviderType> {
    evaluation_context: &'a EvaluationContext,
    agent_execution_context: &'a AgentExecutionContext,
    model_provider: &'a ModelProviderType,
    provider_config: &'a ProviderConfig,
    model_name: &'a str,
    inference: &'a HashMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
struct RenderedPrompt {
    text: String,
    content: Vec<ModelPromptContent>,
}

impl RenderedPrompt {
    fn text(text: String) -> Self {
        let mut rendered_prompt = Self::default();
        rendered_prompt.push_text(text);

        rendered_prompt
    }

    fn assets(assets: Vec<ModelAsset>) -> Self {
        Self {
            text: String::new(),
            content: assets.into_iter().map(ModelPromptContent::Asset).collect(),
        }
    }

    fn push_text(&mut self, text: String) {
        self.text.push_str(&text);

        if text.is_empty() {
            return;
        }

        match self.content.last_mut() {
            Some(ModelPromptContent::Text(existing_text)) => existing_text.push_str(&text),
            Some(ModelPromptContent::Asset(_)) | None => self.content.push(ModelPromptContent::text(text)),
        }
    }

    fn extend(&mut self, rendered_prompt: Self) {
        self.text.push_str(&rendered_prompt.text);

        for prompt_content in rendered_prompt.content {
            match (self.content.last_mut(), prompt_content) {
                (Some(ModelPromptContent::Text(existing_text)), ModelPromptContent::Text(text)) => {
                    existing_text.push_str(&text);
                }
                (_, prompt_content) => self.content.push(prompt_content),
            }
        }
    }

    fn asset_kinds(&self) -> Vec<ModelAssetKind> {
        self.content
            .iter()
            .filter_map(|prompt_content| match prompt_content {
                ModelPromptContent::Asset(asset) => Some(asset.kind),
                ModelPromptContent::Text(_) => None,
            })
            .collect()
    }

    fn into_prompt_content(self, import_context: &str) -> Vec<ModelPromptContent> {
        if import_context.is_empty() {
            return self.content;
        }

        let mut content = Vec::new();
        content.push(ModelPromptContent::text(format!("{import_context}\n\n")));
        content.extend(self.content);

        content
    }
}

impl WorkflowExecutor {
    const DEFAULT_CONTEXT_COMPACTION_INSTRUCTION: &'static str =
        "Compact the prior context into a concise summary for the next agent. Preserve facts, decisions, constraints, tool results, and unresolved questions. Omit redundant wording.";

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

        let prepared_request = self
            .prepare_agent_request(planned_agent, &evaluation_context, agent_execution_context, model_provider)
            .await?;
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

        if let Some(completed_agent_execution) = prepared_request
            .completed_from_cache(
                cache_key.as_ref(),
                planned_agent,
                agent_execution_context,
                agent_started_at,
                agent_run_context.iteration_index,
            )
            .await?
        {
            return Ok(completed_agent_execution);
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

    async fn prepare_agent_request<ModelProviderType>(
        &self,
        planned_agent: &PlannedAgent,
        evaluation_context: &EvaluationContext,
        agent_execution_context: &AgentExecutionContext,
        model_provider: &ModelProviderType,
    ) -> Result<PreparedAgentRequest, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
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
        let context = self
            .resolve_agent_context(
                planned_agent,
                AgentContextResolutionRequest {
                    evaluation_context,
                    agent_execution_context,
                    model_provider,
                    provider_config: &provider_config,
                    model_name: &model_name,
                    inference: &inference,
                },
            )
            .await?;
        let model_declaration = self
            .workflow
            .find_model(&planned_agent.model_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("model profile `{}` is not declared", planned_agent.model_name),
            })?;
        let supported_asset_kinds = model_declaration.supported_asset_kinds()?;
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
        let rendered_instruction = self.evaluate_prompt_expression(
            instruction_expression,
            tool_call_execution_context,
            &format!("instruction for agent `{}`", planned_agent.name),
        )?;
        self.validate_model_asset_support(planned_agent, &supported_asset_kinds, &rendered_instruction)?;
        let agent_instruction = rendered_instruction.text.clone();
        let prompt = if agent_execution_context.import_context.is_empty() {
            agent_instruction.clone()
        } else {
            format!("{}\n\n{agent_instruction}", agent_execution_context.import_context)
        };
        let prompt_content = rendered_instruction.into_prompt_content(agent_execution_context.import_context.as_str());
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
            context,
            prompt,
            prompt_content,
            supported_asset_kinds,
            output_schema,
            output_injections,
            tool_definitions,
            tool_names,
        })
    }

    fn evaluate_prompt_expression(
        &self,
        expression: &Expression,
        tool_call_execution_context: ToolCallExecutionContext<'_>,
        context: &str,
    ) -> Result<RenderedPrompt, ExecutorError> {
        match expression {
            Expression::StringTemplate(string_template) => {
                let mut rendered_prompt = RenderedPrompt::default();

                for string_template_part in &string_template.parts {
                    match string_template_part {
                        superwire_dsl::StringTemplatePart::Text(template_text) => {
                            rendered_prompt.push_text(template_text.clone());
                        }
                        superwire_dsl::StringTemplatePart::Interpolation(interpolation_expression) => {
                            let interpolation_prompt =
                                self.evaluate_prompt_expression(interpolation_expression, tool_call_execution_context, context)?;
                            rendered_prompt.extend(interpolation_prompt);
                        }
                    }
                }

                Ok(rendered_prompt)
            }
            Expression::Asset(_) => {
                let value = self.evaluate_runtime_expression(expression, tool_call_execution_context, context)?;
                let Some(assets) = ModelAsset::all_from_value(&value) else {
                    return Err(ExecutorError::Other {
                        message: format!("asset expression for {context} did not produce asset values"),
                    });
                };

                Ok(RenderedPrompt::assets(assets))
            }
            Expression::Reference(_) => {
                let value = self.evaluate_runtime_expression(expression, tool_call_execution_context, context)?;

                if let Some(assets) = ModelAsset::non_empty_all_from_value(&value) {
                    return Ok(RenderedPrompt::assets(assets));
                }

                Ok(RenderedPrompt::text(normalize_prompt(&value)))
            }
            _ => {
                let value = self.evaluate_runtime_expression(expression, tool_call_execution_context, context)?;

                Ok(RenderedPrompt::text(normalize_prompt(&value)))
            }
        }
    }

    async fn resolve_agent_context<ModelProviderType>(
        &self,
        planned_agent: &PlannedAgent,
        request: AgentContextResolutionRequest<'_, ModelProviderType>,
    ) -> Result<Option<Value>, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let Some(agent_context) = planned_agent.declaration.context_property() else {
            return Ok(None);
        };

        let source_context = self.source_agent_context(agent_context, request.evaluation_context, &planned_agent.name)?;

        match agent_context {
            AgentContext::Direct(_) => Ok(Some(source_context)),
            AgentContext::Compact(_) => {
                let instruction =
                    self.resolve_compaction_instruction(agent_context, request.evaluation_context, request.agent_execution_context)?;
                let output_schema = ModelSchema::workflow(WorkflowType::String);
                let tool_definitions = vec![ModelToolDefinition::finalize(output_schema.clone())];
                let compaction_request = ModelRequest {
                    agent_name: format!("{}__context_compaction", planned_agent.name),
                    provider_config: request.provider_config.clone(),
                    model_name: request.model_name.to_string(),
                    inference: request.inference.clone(),
                    context: Some(source_context),
                    prompt: instruction.clone(),
                    prompt_content: vec![ModelPromptContent::text(instruction)],
                    output_schema,
                    tools: tool_definitions,
                    event_sender: request.agent_execution_context.event_sender.clone(),
                    mcp_pool: self.mcp_pool.clone(),
                    tool_call_tracker: request.agent_execution_context.tool_call_tracker.clone(),
                };
                let compaction_response = request.model_provider.generate(compaction_request).await?;

                Ok(Some(compaction_response.context))
            }
        }
    }

    fn source_agent_context(
        &self,
        agent_context: &AgentContext,
        evaluation_context: &EvaluationContext,
        agent_name: &str,
    ) -> Result<Value, ExecutorError> {
        if !agent_context.reference().is_agent_root() {
            return Err(ExecutorError::Other {
                message: format!("context for agent `{agent_name}` must reference `agent.<name>`"),
            });
        }

        if !agent_context.reference().has_single_access() {
            return Err(ExecutorError::Other {
                message: format!("context for agent `{agent_name}` must reference a whole agent, not an output field"),
            });
        }

        let Some(source_agent_name) = agent_context.agent_name() else {
            return Err(ExecutorError::Other {
                message: format!("context for agent `{agent_name}` must include a source agent name"),
            });
        };

        evaluation_context
            .agent_contexts
            .get(source_agent_name)
            .cloned()
            .ok_or_else(|| ExecutorError::Other {
                message: format!("context for agent `{source_agent_name}` is not available yet"),
            })
    }

    fn resolve_compaction_instruction(
        &self,
        agent_context: &AgentContext,
        evaluation_context: &EvaluationContext,
        agent_execution_context: &AgentExecutionContext,
    ) -> Result<String, ExecutorError> {
        let Some(instruction_expression) = agent_context.instruction() else {
            return Ok(Self::DEFAULT_CONTEXT_COMPACTION_INSTRUCTION.to_string());
        };
        let tool_call_execution_context = ToolCallExecutionContext::new(
            evaluation_context,
            agent_execution_context.event_sender.as_ref(),
            &agent_execution_context.tool_call_tracker,
        );
        let rendered_instruction = self.evaluate_prompt_expression(
            instruction_expression,
            tool_call_execution_context,
            "context compaction instruction",
        )?;

        Ok(rendered_instruction.text)
    }

    fn validate_model_asset_support(
        &self,
        planned_agent: &PlannedAgent,
        supported_asset_kinds: &[ModelAssetKind],
        rendered_instruction: &RenderedPrompt,
    ) -> Result<(), ExecutorError> {
        for asset_kind in rendered_instruction.asset_kinds() {
            if supported_asset_kinds.contains(&asset_kind) {
                continue;
            }

            return Err(ExecutorError::Other {
                message: format!(
                    "agent `{}` includes a {} asset, but model `{}` does not declare support for it with `assets`",
                    planned_agent.name,
                    asset_kind.as_str(),
                    planned_agent.model_name
                ),
            });
        }

        Ok(())
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
