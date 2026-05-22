use super::{AgentExecutionContext, AgentRunContext, CompletedAgentExecution, ExecutorError, WorkflowExecutor};
use crate::event::ExecutorEvent;
use crate::model::ModelProvider;
use crate::runtime::state::RuntimeState;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::{Map, Value};
use std::time::Instant;
use superwire_core::dsl::AgentForLoopPattern;
use superwire_core::semantic::support::expression::evaluate_expression;
use superwire_core::semantic::support::types::value_kind_name;
use superwire_core::semantic::PlannedAgent;

impl WorkflowExecutor {
    pub(in crate::runtime) async fn execute_for_loop_agent<ModelProviderType>(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &RuntimeState,
        model_provider: &ModelProviderType,
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
        let loop_pattern = &for_loop.pattern;
        let evaluation_context = runtime_state.evaluation_context();
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
        let loop_started_at = Instant::now();
        let iteration_bindings = loop_pattern.iteration_bindings(items)?;

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_loop_started(planned_agent.name.clone(), iteration_bindings))
                .await;
        }

        if items.is_empty() {
            if let Some(event_sender) = &agent_execution_context.event_sender {
                let _ = event_sender
                    .send(ExecutorEvent::agent_loop_completed(
                        planned_agent.name.clone(),
                        Value::Array(Vec::new()),
                        loop_started_at.elapsed(),
                        0,
                    ))
                    .await;
            }

            return Ok(CompletedAgentExecution {
                agent_name: planned_agent.name.clone(),
                output: Value::Array(Vec::new()),
                context: Value::Null,
            });
        }

        let mut pending_iterations = FuturesUnordered::new();
        let agent_name = planned_agent.name.clone();
        let iteration_count = items.len();
        let tool_call_tracker = runtime_state.tool_call_tracker();

        for (iteration_index, item) in items.iter().enumerate() {
            let mut iteration_state = runtime_state.clone();
            loop_pattern.bind_loop_variables(item, &mut iteration_state)?;

            let iteration_execution_context = AgentExecutionContext {
                event_sender: agent_execution_context.event_sender.clone(),
                import_context: agent_execution_context.import_context.clone(),
                tool_call_tracker: tool_call_tracker.clone(),
                runtime_concurrency_limiter: agent_execution_context.runtime_concurrency_limiter.clone(),
            };
            let runtime_concurrency_limiter = iteration_execution_context.runtime_concurrency_limiter.clone();

            pending_iterations.push(async move {
                runtime_concurrency_limiter
                    .run(self.execute_agent(AgentRunContext {
                        planned_agent,
                        runtime_state: &iteration_state,
                        model_provider,
                        agent_execution_context: &iteration_execution_context,
                        iteration_index: Some(iteration_index),
                    }))
                    .await
                    .map(|completed_execution| (iteration_index, completed_execution))
            });
        }

        let mut iteration_outputs = vec![Value::Null; iteration_count];

        while let Some(iteration_result) = pending_iterations.next().await {
            let (iteration_index, completed_execution) = iteration_result?;
            iteration_outputs[iteration_index] = completed_execution.output;
        }
        let output = Value::Array(iteration_outputs);

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_loop_completed(
                    agent_name.clone(),
                    output.clone(),
                    loop_started_at.elapsed(),
                    iteration_count,
                ))
                .await;
        }

        Ok(CompletedAgentExecution {
            agent_name,
            output,
            context: Value::Null,
        })
    }
}

trait AgentForLoopPatternRuntimeExt {
    fn bind_loop_variables(&self, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError>;
    fn iteration_bindings(&self, items: &[Value]) -> Result<Vec<Value>, ExecutorError>;
    fn bindings_for_item(&self, item: &Value) -> Result<Map<String, Value>, ExecutorError>;
}

impl AgentForLoopPatternRuntimeExt for AgentForLoopPattern {
    fn bind_loop_variables(&self, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError> {
        for (binding_name, binding_value) in self.bindings_for_item(item)? {
            runtime_state.insert_local_binding(binding_name, binding_value);
        }

        Ok(())
    }

    fn iteration_bindings(&self, items: &[Value]) -> Result<Vec<Value>, ExecutorError> {
        items
            .iter()
            .enumerate()
            .map(|(iteration_index, item)| {
                let bindings = self.bindings_for_item(item)?;

                Ok(serde_json::json!({
                    "iteration_index": iteration_index,
                    "bindings": bindings,
                }))
            })
            .collect()
    }

    fn bindings_for_item(&self, item: &Value) -> Result<Map<String, Value>, ExecutorError> {
        let mut bindings = Map::new();

        match self {
            AgentForLoopPattern::Identifier(identifier) => {
                bindings.insert(identifier.clone(), item.clone());
            }
            AgentForLoopPattern::ObjectDestructuring(field_names) => {
                let item_object = item.as_object().ok_or_else(|| ExecutorError::Other {
                    message: format!("for-loop destructuring expects object, found {}", value_kind_name(item)),
                })?;
                for field_name in field_names {
                    let field_value = item_object.get(field_name).cloned().unwrap_or(Value::Null);
                    bindings.insert(field_name.clone(), field_value);
                }
            }
        }

        Ok(bindings)
    }
}
