use super::{AgentExecutionContext, AgentRunContext, CompletedAgentExecution, ExecutorError, WorkflowExecutor};
use crate::model::ModelProvider;
use crate::runtime::state::RuntimeState;
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use superwire_core::dsl::AgentForLoopPattern;
use superwire_core::semantic::support::expression::evaluate_expression;
use superwire_core::semantic::support::types::value_kind_name;
use superwire_core::semantic::PlannedAgent;
use tokio::sync::Semaphore;

impl WorkflowExecutor {
    pub(in crate::runtime) async fn execute_for_loop_agent<ModelProviderType>(
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
            loop_pattern.bind_loop_variables(item, &mut iteration_state)?;

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
                    .execute_agent(AgentRunContext {
                        planned_agent: &agent_clone,
                        runtime_state: &iteration_state,
                        model_provider,
                        agent_execution_context: &iteration_execution_context,
                    })
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
}

trait AgentForLoopPatternRuntimeExt {
    fn bind_loop_variables(&self, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError>;
}

impl AgentForLoopPatternRuntimeExt for AgentForLoopPattern {
    fn bind_loop_variables(&self, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError> {
        match self {
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
}
