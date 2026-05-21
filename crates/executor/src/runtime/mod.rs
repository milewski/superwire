mod build;
mod configuration;
pub mod error;
mod mcp;
mod schema;
pub mod state;
mod tools;

pub use error::ExecutorError;
pub(in crate::runtime) use schema::value_object;

use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, ModelRequest, ModelToolDefinition, ToolCallTracker};
use crate::runtime::mcp::normalize_prompt;
use crate::runtime::schema::PlannedAgentSchemaExt;
use crate::runtime::state::RuntimeState;
use crate::runtime::tools::ExpressionMcpExecutionPlanExt;
use futures::future::try_join_all;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use superwire_core::dsl::{AgentExpressionPropertyName, AgentForLoopPattern, AgentProperty, Declaration, Expression, Workflow};
use superwire_core::mcp::McpClientPool;
use superwire_core::semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_core::semantic::support::types::value_kind_name;
use superwire_core::semantic::{ExecutionPlan, PlannedAgent, WorkflowExecutionGraph, WorkflowSemanticError};
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Clone)]
struct CompletedAgentExecution {
    agent_name: String,
    output: Value,
    context: Value,
}

impl CompletedAgentExecution {
    fn apply_to_runtime_state(self, runtime_state: &mut RuntimeState) {
        runtime_state.insert_agent_result(self.agent_name, self.output, self.context);
    }
}

#[derive(Debug)]
pub struct WorkflowExecutor {
    workflow: Workflow,
    execution_plan: ExecutionPlan,
    mcp_pool: McpClientPool,
}

#[derive(Debug, Clone)]
struct AgentExecutionContext {
    event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    import_context: String,
    tool_call_tracker: ToolCallTracker,
}

impl WorkflowExecutor {
    #[must_use]
    pub fn agent_execution_order(&self) -> Vec<String> {
        self.execution_plan.agent_execution_order.clone()
    }

    #[must_use]
    pub fn mcp_imports(&self) -> &[superwire_core::semantic::PlannedMcpImport] {
        &self.execution_plan.mcp_imports
    }

    #[must_use]
    pub fn execution_graph(&self) -> WorkflowExecutionGraph {
        self.execution_plan.execution_graph(&self.workflow)
    }

    pub fn planned_execution_steps(&self, input: &Value, secrets: &Value, max_concurrency: usize) -> Result<Value, ExecutorError> {
        let input_values = self.resolve_input_values(input)?;
        let secret_values = self.resolve_secret_values(secrets)?;
        let runtime_state = RuntimeState::new(input_values, secret_values);
        let evaluation_context = runtime_state.evaluation_context(HashMap::new());
        let mut steps = Vec::new();

        let dynamic_calls = self.planned_workflow_dynamic_calls(&evaluation_context)?;

        if !dynamic_calls.is_empty() {
            steps.push(serde_json::json!({
                "type": "workflow_dynamic",
                "calls": dynamic_calls,
            }));
        }

        for execution_batch in self.resolve_agent_execution_batches()? {
            let planned_agents = execution_batch
                .iter()
                .map(|agent_name| self.planned_agent_step(agent_name, &evaluation_context))
                .collect::<Result<Vec<_>, _>>()?;

            if planned_agents.len() == 1 || max_concurrency <= 1 {
                steps.extend(planned_agents);
            } else {
                steps.push(serde_json::json!({ "parallel": planned_agents }));
            }
        }

        let output_calls = self.planned_output_calls(&evaluation_context)?;

        steps.push(serde_json::json!({
            "type": "workflow_output",
            "calls": output_calls,
        }));

        Ok(Value::Array(steps))
    }

    fn planned_workflow_dynamic_calls(&self, evaluation_context: &EvaluationContext) -> Result<Vec<Value>, ExecutorError> {
        let mut calls = Vec::new();

        for declaration in self.workflow.declarations() {
            let Declaration::Dynamic(dynamic_block) = declaration else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                calls.extend(dynamic_field.value.planned_mcp_calls(self, evaluation_context)?);
            }
        }

        Ok(calls)
    }

    fn planned_output_calls(&self, evaluation_context: &EvaluationContext) -> Result<Vec<Value>, ExecutorError> {
        let mut calls = Vec::new();

        for output_field in &self.execution_plan.output_declaration.fields {
            calls.extend(output_field.value.planned_mcp_calls(self, evaluation_context)?);
        }

        Ok(calls)
    }

    fn planned_agent_step(&self, agent_name: &str, evaluation_context: &EvaluationContext) -> Result<Value, ExecutorError> {
        let planned_agent = self
            .execution_plan
            .planned_agents
            .get(agent_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("planned agent `{agent_name}` is missing"),
            })?;
        let mut dynamic_calls = Vec::new();

        for agent_property in &planned_agent.declaration.properties {
            let AgentProperty::Dynamic(dynamic_block) = agent_property else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                dynamic_calls.extend(dynamic_field.value.planned_mcp_calls(self, evaluation_context)?);
            }
        }

        Ok(serde_json::json!({
            "type": "agent",
            "agent_name": agent_name,
            "dependencies": planned_agent.dependencies,
            "dynamic_calls": dynamic_calls,
            "available_mcp_calls": self.planned_agent_available_mcp_calls(planned_agent, evaluation_context)?,
        }))
    }

    pub async fn execute<ModelProviderType>(
        &self,
        input: Value,
        secrets: Value,
        model_provider: &ModelProviderType,
        event_sender: Option<mpsc::Sender<ExecutorEvent>>,
        max_concurrency: usize,
    ) -> Result<Value, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let input_values = self.resolve_input_values(&input)?;
        let secret_values = self.resolve_secret_values(&secrets)?;
        let mut runtime_state = RuntimeState::new(input_values, secret_values);
        let tool_call_tracker = ToolCallTracker::default();

        log::info!("executing workflow runtime");

        self.execute_workflow_dynamic_blocks(&mut runtime_state, event_sender.as_ref(), &tool_call_tracker)?;

        let import_context = self.resolve_mcp_import_context(&runtime_state.evaluation_context(HashMap::new()))?;

        log::debug!(
            "workflow-level import context resolved: {}",
            if import_context.is_empty() { "empty" } else { "populated" }
        );

        for execution_batch in self.resolve_agent_execution_batches()? {
            let runtime_state_snapshot = runtime_state.clone();
            let mut for_loop_agents = Vec::new();
            let mut regular_agents = Vec::new();

            log::debug!("starting execution batch: agents={execution_batch:?}");

            for agent_name in execution_batch {
                let planned_agent = self
                    .execution_plan
                    .planned_agents
                    .get(&agent_name)
                    .expect("planned agent should exist")
                    .clone();

                if planned_agent.declaration.for_loop.is_some() {
                    for_loop_agents.push(planned_agent);
                } else {
                    regular_agents.push(planned_agent);
                }
            }

            let agent_execution_context = AgentExecutionContext {
                event_sender: event_sender.clone(),
                import_context: import_context.clone(),
                tool_call_tracker: tool_call_tracker.clone(),
            };

            for planned_agent in for_loop_agents {
                let completed = self
                    .execute_for_loop_agent(
                        planned_agent,
                        &runtime_state_snapshot,
                        model_provider,
                        max_concurrency,
                        &agent_execution_context,
                    )
                    .await?;
                completed.apply_to_runtime_state(&mut runtime_state);
            }

            let mut pending_executions = Vec::new();

            for planned_agent in regular_agents {
                let runtime_state_snapshot = runtime_state_snapshot.clone();
                let agent_execution_context = agent_execution_context.clone();

                pending_executions.push(async move {
                    self.execute_agent(&planned_agent, &runtime_state_snapshot, model_provider, &agent_execution_context)
                        .await
                });
            }

            let completed_executions = try_join_all(pending_executions).await?;

            for completed_execution in completed_executions {
                completed_execution.apply_to_runtime_state(&mut runtime_state);
            }
        }

        let output = self.evaluate_workflow_output(&runtime_state, event_sender.as_ref(), &tool_call_tracker)?;
        self.validate_workflow_output_value(&output)?;

        log::info!("workflow runtime completed");

        Ok(output)
    }

    fn resolve_agent_execution_batches(&self) -> Result<Vec<Vec<String>>, ExecutorError> {
        let execution_order = &self.execution_plan.agent_execution_order;
        let mut unresolved_agents = execution_order.iter().cloned().collect::<HashSet<_>>();
        let mut resolved_agents = HashSet::<String>::new();
        let mut execution_batches = Vec::<Vec<String>>::new();

        while !unresolved_agents.is_empty() {
            let mut ready_agents = Vec::<String>::new();

            for agent_name in execution_order {
                if !unresolved_agents.contains(agent_name) {
                    continue;
                }

                let planned_agent = self
                    .execution_plan
                    .planned_agents
                    .get(agent_name)
                    .expect("planned agent should exist");

                if planned_agent
                    .dependencies
                    .iter()
                    .any(|dependency_name| !resolved_agents.contains(dependency_name))
                {
                    continue;
                }

                ready_agents.push(agent_name.clone());
            }

            if ready_agents.is_empty() {
                return Err(ExecutorError::Other {
                    message: "failed to resolve execution batches".to_string(),
                });
            }

            for ready_agent_name in &ready_agents {
                unresolved_agents.remove(ready_agent_name);
                resolved_agents.insert(ready_agent_name.clone());
            }

            execution_batches.push(ready_agents);
        }

        Ok(execution_batches)
    }

    fn execute_workflow_dynamic_blocks(
        &self,
        runtime_state: &mut RuntimeState,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<(), ExecutorError> {
        for declaration in self.workflow.declarations() {
            let Declaration::Dynamic(dynamic_block) = declaration else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                let field_value = self.evaluate_runtime_expression(
                    &dynamic_field.value,
                    &runtime_state.evaluation_context(HashMap::new()),
                    &format!("dynamic field `{}`", dynamic_field.name),
                    event_sender,
                    tool_call_tracker,
                )?;
                runtime_state.insert_local_binding(dynamic_field.name.clone(), field_value);
            }
        }

        Ok(())
    }

    async fn execute_agent<ModelProviderType>(
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
        let model_name = evaluate_agent_model_name(&planned_agent.model_id_expression, &planned_agent.name, &evaluation_context)?;
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

    async fn execute_for_loop_agent<ModelProviderType>(
        &self,
        planned_agent: PlannedAgent,
        runtime_state: &RuntimeState,
        model_provider: &ModelProviderType,
        max_concurrency: usize,
        agent_execution_context: &AgentExecutionContext,
    ) -> Result<CompletedAgentExecution, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let for_loop = planned_agent
            .declaration
            .for_loop
            .as_ref()
            .expect("for-loop agent must have for_loop");
        let loop_pattern = for_loop.pattern.clone();
        let evaluation_context = runtime_state.evaluation_context(HashMap::new());
        let iterable_value = evaluate_expression(
            &for_loop.iterable,
            &evaluation_context,
            &format!("for-loop iterable for agent `{}`", planned_agent.name),
        )?;
        let items = iterable_value.as_array().ok_or_else(|| ExecutorError::Other {
            message: format!(
                "for-loop iterable for agent `{}` must evaluate to an array, found {}",
                planned_agent.name,
                value_kind_name(&iterable_value)
            ),
        })?;

        if items.is_empty() {
            return Ok(CompletedAgentExecution {
                agent_name: planned_agent.name.clone(),
                output: Value::Array(Vec::new()),
                context: Value::Null,
            });
        }

        let concurrency_limit = max_concurrency.max(1);
        let semaphore = Arc::new(Semaphore::new(concurrency_limit));
        let mut pending_iterations = FuturesUnordered::new();
        let agent_name = planned_agent.name.clone();
        let tool_call_tracker = runtime_state.tool_call_tracker();

        for item in items {
            let mut iteration_state = runtime_state.clone();
            Self::bind_loop_variables(&loop_pattern, item, &mut iteration_state)?;
            let semaphore_clone = semaphore.clone();
            let agent_clone = planned_agent.clone();
            let iteration_execution_context = AgentExecutionContext {
                event_sender: agent_execution_context.event_sender.clone(),
                import_context: agent_execution_context.import_context.clone(),
                tool_call_tracker: tool_call_tracker.clone(),
            };

            pending_iterations.push(async move {
                let permit = semaphore_clone.acquire_owned().await.map_err(|error| ExecutorError::Other {
                    message: format!("failed to acquire concurrency permit: {error}"),
                })?;
                let result = self
                    .execute_agent(&agent_clone, &iteration_state, model_provider, &iteration_execution_context)
                    .await;
                drop(permit);
                result
            });
        }

        let mut iteration_outputs = Vec::with_capacity(pending_iterations.len());

        while let Some(iteration_result) = pending_iterations.next().await {
            iteration_outputs.push(iteration_result?.output);
        }

        Ok(CompletedAgentExecution {
            agent_name,
            output: Value::Array(iteration_outputs),
            context: Value::Null,
        })
    }

    fn bind_loop_variables(pattern: &AgentForLoopPattern, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError> {
        match pattern {
            AgentForLoopPattern::Identifier(identifier) => {
                runtime_state.insert_local_binding(identifier.clone(), item.clone());
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                let item_object = item.as_object().ok_or_else(|| ExecutorError::Other {
                    message: format!("for-loop destructuring expects object, found {}", value_kind_name(item)),
                })?;

                for field_name in field_names {
                    let field_value = item_object.get(field_name).cloned().unwrap_or(Value::Null);
                    runtime_state.insert_local_binding(field_name.clone(), field_value);
                }
            }
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

    fn evaluate_runtime_expression(
        &self,
        expression: &Expression,
        evaluation_context: &EvaluationContext,
        context: &str,
        event_sender: Option<&mpsc::Sender<ExecutorEvent>>,
        tool_call_tracker: &ToolCallTracker,
    ) -> Result<Value, ExecutorError> {
        match expression {
            Expression::StringLiteral(string_literal) => Ok(Value::String(string_literal.clone())),
            Expression::StringTemplate(string_template) => {
                let mut rendered_template = String::new();

                for string_template_part in &string_template.parts {
                    match string_template_part {
                        superwire_core::dsl::StringTemplatePart::Text(template_text) => rendered_template.push_str(template_text),
                        superwire_core::dsl::StringTemplatePart::Interpolation(interpolation_expression) => {
                            let interpolation_value = self.evaluate_runtime_expression(
                                interpolation_expression,
                                evaluation_context,
                                context,
                                event_sender,
                                tool_call_tracker,
                            )?;
                            rendered_template.push_str(&normalize_prompt(interpolation_value));
                        }
                    }
                }

                Ok(Value::String(rendered_template))
            }
            Expression::NumberLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral
            | Expression::Reference(_)
            | Expression::FunctionCall(_)
            | Expression::VariantProjection(_)
            | Expression::Match(_) => Ok(evaluate_expression(expression, evaluation_context, context)?),
            Expression::NullFallback(null_fallback) => {
                let value =
                    self.evaluate_runtime_expression(&null_fallback.value, evaluation_context, context, event_sender, tool_call_tracker)?;

                if value.is_null() {
                    return self.evaluate_runtime_expression(
                        &null_fallback.fallback,
                        evaluation_context,
                        context,
                        event_sender,
                        tool_call_tracker,
                    );
                }

                Ok(value)
            }
            Expression::ToolCall(tool_call) => {
                self.execute_deterministic_tool_call(tool_call, evaluation_context, event_sender, tool_call_tracker)
            }
            Expression::McpCall(mcp_call) => self.execute_mcp_call(mcp_call, evaluation_context, event_sender),
            Expression::ArrayLiteral(array_items) => {
                let mut evaluated_items = Vec::with_capacity(array_items.len());

                for array_item in array_items {
                    evaluated_items.push(self.evaluate_runtime_expression(
                        array_item,
                        evaluation_context,
                        context,
                        event_sender,
                        tool_call_tracker,
                    )?);
                }

                Ok(Value::Array(evaluated_items))
            }
            Expression::ObjectLiteral(object_fields) => {
                let mut evaluated_fields = Map::new();

                for object_field in object_fields {
                    let field_value = self.evaluate_runtime_expression(
                        &object_field.value,
                        evaluation_context,
                        context,
                        event_sender,
                        tool_call_tracker,
                    )?;
                    evaluated_fields.insert(object_field.name.clone(), field_value);
                }

                Ok(Value::Object(evaluated_fields))
            }
        }
    }
}

fn evaluate_agent_model_name(
    model_expression: &Expression,
    agent_name: &str,
    evaluation_context: &EvaluationContext,
) -> Result<String, ExecutorError> {
    let model_value = evaluate_expression(model_expression, evaluation_context, &format!("model for agent `{agent_name}`"))?;

    model_value.as_str().map(str::to_string).ok_or_else(|| ExecutorError::Other {
        message: format!("model for agent `{agent_name}` must resolve to string"),
    })
}
