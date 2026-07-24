use super::{AgentExecutionContext, AgentRunContext, CompletedAgentExecution, ExecutorError, WorkflowExecutor};
use crate::model::{ExecutorEventSenderExt, ModelProvider};
use crate::runtime::state::RuntimeState;
use futures::stream::{self, StreamExt};
use serde_json::{Map, Value};
use std::time::Instant;
use superwire_dsl::AgentForLoopPattern;
use superwire_protocol::event::{ExecutorDiagnostic, ExecutorDiagnosticCode, ExecutorDiagnosticSubject, ExecutorEvent, ExecutorStage};
use superwire_semantic::support::expression::evaluate_expression;
use superwire_semantic::support::types::value_kind_name;
use superwire_semantic::PlannedAgent;

const MAX_FOR_LOOP_ITERATIONS: usize = 1024;
const MAX_ACTIVE_LOOP_ITERATIONS: usize = 64;

struct AgentLoopLifecycle {
    event_sender: Option<tokio::sync::mpsc::Sender<ExecutorEvent>>,
    agent_name: String,
    started_at: Instant,
    terminal: bool,
}

impl AgentLoopLifecycle {
    fn new(agent_name: String, event_sender: Option<tokio::sync::mpsc::Sender<ExecutorEvent>>, started_at: Instant) -> Self {
        Self {
            event_sender,
            agent_name,
            started_at,
            terminal: false,
        }
    }

    fn mark_terminal(&mut self) {
        self.terminal = true;
    }
}

impl Drop for AgentLoopLifecycle {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }

        let Some(event_sender) = &self.event_sender else {
            return;
        };
        let subject = ExecutorDiagnosticSubject::Agent {
            agent_name: self.agent_name.clone(),
            iteration_index: None,
        };
        let event = if std::thread::panicking() {
            let diagnostic = ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::InternalPanic,
                ExecutorStage::Agent,
                format!("agent loop `{}` panicked before a terminal event", self.agent_name),
                subject,
            );

            ExecutorEvent::agent_loop_failed(self.agent_name.clone(), diagnostic, self.started_at.elapsed())
        } else {
            let diagnostic = ExecutorDiagnostic::error(
                ExecutorDiagnosticCode::Cancelled,
                ExecutorStage::Agent,
                format!("agent loop `{}` was cancelled before a terminal event", self.agent_name),
                subject,
            );

            ExecutorEvent::agent_loop_cancelled(self.agent_name.clone(), diagnostic, self.started_at.elapsed())
        };

        event_sender.try_send_observed(event);
    }
}

impl WorkflowExecutor {
    #[allow(clippy::too_many_lines)]
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

        if items.len() > MAX_FOR_LOOP_ITERATIONS {
            return Err(ExecutorError::invalid_input(format!(
                "for-loop agent `{}` has {} iterations, exceeding the limit of {MAX_FOR_LOOP_ITERATIONS}",
                planned_agent.name,
                items.len()
            )));
        }

        let loop_started_at = Instant::now();

        if let Some(event_sender) = &agent_execution_context.event_sender {
            let binding_names = loop_pattern.bound_identifier_names().into_iter().map(str::to_owned).collect();

            event_sender
                .send_observed(ExecutorEvent::agent_loop_started(
                    planned_agent.name.clone(),
                    items.len(),
                    binding_names,
                ))
                .await;
        }

        let mut loop_lifecycle = AgentLoopLifecycle::new(
            planned_agent.name.clone(),
            agent_execution_context.event_sender.clone(),
            loop_started_at,
        );

        if items.is_empty() {
            if let Some(event_sender) = &agent_execution_context.event_sender {
                event_sender
                    .send_observed(ExecutorEvent::agent_loop_completed(
                        planned_agent.name.clone(),
                        loop_started_at.elapsed(),
                        0,
                    ))
                    .await;
            }

            loop_lifecycle.mark_terminal();

            return Ok(CompletedAgentExecution {
                agent_name: planned_agent.name.clone(),
                output: Value::Array(Vec::new()),
                context: Value::Null,
            });
        }

        let agent_name = planned_agent.name.clone();
        let iteration_count = items.len();
        let tool_call_tracker = runtime_state.tool_call_tracker();
        let mut pending_iterations = stream::iter(items.iter().cloned().enumerate())
            .map(|(iteration_index, item)| {
                let mut iteration_state = runtime_state.clone();
                let binding_result = loop_pattern.bind_loop_variables(&item, &mut iteration_state);
                let iteration_execution_context = AgentExecutionContext {
                    event_sender: agent_execution_context.event_sender.clone(),
                    import_context: agent_execution_context.import_context.clone(),
                    tool_call_tracker: tool_call_tracker.clone(),
                    runtime_concurrency_limiter: agent_execution_context.runtime_concurrency_limiter.clone(),
                    cache_options: agent_execution_context.cache_options.clone(),
                };
                let runtime_concurrency_limiter = iteration_execution_context.runtime_concurrency_limiter.clone();

                async move {
                    binding_result?;

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
                }
            })
            .buffer_unordered(MAX_ACTIVE_LOOP_ITERATIONS.min(iteration_count));
        let mut iteration_outputs = vec![Value::Null; iteration_count];

        while let Some(iteration_result) = pending_iterations.next().await {
            match iteration_result {
                Ok((iteration_index, completed_execution)) => {
                    iteration_outputs[iteration_index] = completed_execution.output;
                }
                Err(error) => {
                    if let Some(event_sender) = &agent_execution_context.event_sender {
                        event_sender
                            .send_observed(ExecutorEvent::agent_loop_failed(
                                agent_name,
                                error.diagnostic(),
                                loop_started_at.elapsed(),
                            ))
                            .await;
                    }

                    loop_lifecycle.mark_terminal();

                    return Err(error);
                }
            }
        }
        let output = Value::Array(iteration_outputs);

        if let Some(event_sender) = &agent_execution_context.event_sender {
            event_sender
                .send_observed(ExecutorEvent::agent_loop_completed(
                    agent_name.clone(),
                    loop_started_at.elapsed(),
                    iteration_count,
                ))
                .await;
        }

        loop_lifecycle.mark_terminal();

        Ok(CompletedAgentExecution {
            agent_name,
            output,
            context: Value::Null,
        })
    }
}

trait AgentForLoopPatternRuntimeExt {
    fn bind_loop_variables(&self, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError>;
    fn bindings_for_item(&self, item: &Value) -> Result<Map<String, Value>, ExecutorError>;
}

impl AgentForLoopPatternRuntimeExt for AgentForLoopPattern {
    fn bind_loop_variables(&self, item: &Value, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError> {
        for (binding_name, binding_value) in self.bindings_for_item(item)? {
            runtime_state.insert_local_binding(binding_name, binding_value);
        }

        Ok(())
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
