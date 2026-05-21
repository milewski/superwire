use super::{AgentExecutionContext, CompletedAgentExecution, ExecutorError, WorkflowExecutor};
use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, ModelRequest, ModelToolDefinition, ToolCallTracker};
use crate::runtime::mcp::normalize_prompt;
use crate::runtime::schema::PlannedAgentSchemaExt;
use crate::runtime::state::RuntimeState;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;
use superwire_core::dsl::{AgentExpressionPropertyName, AgentProperty};
use superwire_core::semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_core::semantic::{PlannedAgent, WorkflowSemanticError};
use tokio::sync::mpsc;

impl WorkflowExecutor {
    pub(in crate::runtime) async fn execute_agent<ModelProviderType>(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &RuntimeState,
        model_provider: &ModelProviderType,
        agent_execution_context: &AgentExecutionContext,
    ) -> Result<CompletedAgentExecution, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let agent_started_at = Instant::now();
        let agent_dynamic_values = self.execute_agent_dynamic_blocks(
            planned_agent,
            runtime_state,
            agent_execution_context.event_sender.as_ref(),
            &agent_execution_context.tool_call_tracker,
        )?;
        let evaluation_context = runtime_state.evaluation_context(agent_dynamic_values);

        log::info!("starting agent `{}`", planned_agent.name);

        let provider_template = self
            .execution_plan
            .provider_index
            .get(&planned_agent.provider_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("provider `{}` is not declared", planned_agent.provider_name),
            })?;
        let provider_config = provider_template.resolve(&planned_agent.provider_name, &evaluation_context)?;
        let model_name = planned_agent.evaluate_model_name(&evaluation_context)?;
        let inference = self.evaluate_inference_fields(planned_agent, &evaluation_context)?;
        let instruction_expression = planned_agent
            .declaration
            .required_expression_property(AgentExpressionPropertyName::Instruction)
            .map_err(|missing_property| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: planned_agent.name.clone(),
                property: missing_property.as_str().to_string(),
                message: "property is required".to_string(),
            })?;
        let agent_instruction = normalize_prompt(self.evaluate_runtime_expression(
            instruction_expression,
            &evaluation_context,
            &format!("instruction for agent `{}`", planned_agent.name),
            None,
            &agent_execution_context.tool_call_tracker,
        )?);
        let prompt = if agent_execution_context.import_context.is_empty() {
            agent_instruction
        } else {
            format!("{}\n\n{agent_instruction}", agent_execution_context.import_context)
        };
        let mut tool_definitions = self.resolve_agent_use_definitions(planned_agent, &evaluation_context)?;
        let output_schema = planned_agent.push_finalize_tool_definition(&mut tool_definitions);
        let tool_names = tool_definitions
            .iter()
            .map(ModelToolDefinition::event_display_name)
            .collect::<Vec<_>>();

        log::debug!(
            "agent `{}` request prepared: model={}, tools={}, response_schema={}",
            planned_agent.name,
            model_name,
            tool_definitions.len(),
            output_schema.get("type").and_then(Value::as_str).unwrap_or("unknown")
        );

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_started(
                    planned_agent.name.clone(),
                    model_name.clone(),
                    tool_names,
                ))
                .await;
        }

        let model_response = model_provider
            .generate(ModelRequest {
                agent_name: planned_agent.name.clone(),
                provider_config,
                model_name,
                inference,
                prompt,
                output_schema,
                tools: tool_definitions,
                event_sender: agent_execution_context.event_sender.clone(),
                mcp_pool: self.mcp_pool.clone(),
                tool_call_tracker: agent_execution_context.tool_call_tracker.clone(),
            })
            .await?;

        log::debug!("agent `{}` model response received", planned_agent.name);

        planned_agent.validate_output_value(&model_response.output)?;

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_completed(
                    planned_agent.name.clone(),
                    model_response.output.clone(),
                    agent_started_at.elapsed(),
                ))
                .await;
        }

        Ok(CompletedAgentExecution {
            agent_name: planned_agent.name.clone(),
            output: model_response.output,
            context: model_response.context,
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
                let field_value = self.evaluate_runtime_expression(
                    &dynamic_field.value,
                    &runtime_state.evaluation_context(dynamic_values.clone()),
                    &format!("dynamic field `{}` for agent `{}`", dynamic_field.name, planned_agent.name),
                    event_sender,
                    tool_call_tracker,
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
    ) -> Result<BTreeMap<String, Value>, ExecutorError> {
        let mut inference = BTreeMap::new();

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
