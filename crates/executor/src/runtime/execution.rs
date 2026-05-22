use super::{
    AgentExecutionContext, AgentRunContext, ExecutorError, RuntimeConcurrencyLimiter, RuntimeValidationContext, ToolCallExecutionContext,
    WorkflowExecutor,
};
use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, ToolCallTracker};
use crate::runtime::state::RuntimeState;
use futures::future::try_join_all;
use serde_json::Value;
use superwire_core::dsl::Declaration;
use tokio::sync::mpsc;

impl WorkflowExecutor {
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
        let runtime_configuration = self.resolve_runtime_configuration(RuntimeValidationContext {
            input: &input,
            secrets: &secrets,
        })?;
        let mut runtime_state = RuntimeState::new(runtime_configuration.input_values, runtime_configuration.secret_values);
        let tool_call_tracker = ToolCallTracker::default();
        let runtime_concurrency_limiter = RuntimeConcurrencyLimiter::new(max_concurrency);

        log::info!("executing workflow runtime");

        self.execute_workflow_dynamic_blocks(&mut runtime_state, event_sender.as_ref(), &tool_call_tracker)?;

        let import_context = self.resolve_mcp_import_context(&runtime_state.evaluation_context())?;

        log::debug!(
            "workflow-level import context resolved: {}",
            if import_context.is_empty() { "empty" } else { "populated" }
        );

        for execution_batch in self.execution_plan.agent_execution_batches()? {
            let runtime_state_snapshot = runtime_state.clone();
            let mut for_loop_agents = Vec::new();
            let mut regular_agents = Vec::new();

            log::debug!("starting execution batch: agents={execution_batch:?}");

            for agent_name in execution_batch {
                let planned_agent = self
                    .execution_plan
                    .planned_agents
                    .get(&agent_name)
                    .expect("planned agent should exist");

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
                runtime_concurrency_limiter: runtime_concurrency_limiter.clone(),
            };

            for planned_agent in for_loop_agents {
                let completed_execution = self
                    .execute_for_loop_agent(planned_agent, &runtime_state_snapshot, model_provider, &agent_execution_context)
                    .await?;
                completed_execution.apply_to_runtime_state(&mut runtime_state);
            }

            let mut pending_executions = Vec::new();

            for planned_agent in regular_agents {
                let runtime_state_snapshot = runtime_state_snapshot.clone();
                let agent_execution_context = agent_execution_context.clone();
                let runtime_concurrency_limiter = agent_execution_context.runtime_concurrency_limiter.clone();

                pending_executions.push(async move {
                    runtime_concurrency_limiter
                        .run(self.execute_agent(AgentRunContext {
                            planned_agent,
                            runtime_state: &runtime_state_snapshot,
                            model_provider,
                            agent_execution_context: &agent_execution_context,
                        }))
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
                let evaluation_context = runtime_state.evaluation_context();
                let tool_call_execution_context = ToolCallExecutionContext::new(&evaluation_context, event_sender, tool_call_tracker);
                let field_value = self.evaluate_runtime_expression(
                    &dynamic_field.value,
                    tool_call_execution_context,
                    &format!("dynamic field `{}`", dynamic_field.name),
                )?;
                runtime_state.insert_local_binding(dynamic_field.name.clone(), field_value);
            }
        }

        Ok(())
    }
}
