pub mod error;
pub mod state;

pub use error::ExecutorError;

use crate::event::ExecutorEvent;
use crate::model::{ModelProvider, ModelRequest};
use crate::runtime::state::RuntimeState;
use futures::future::try_join_all;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use superwire_core::dsl::{parse_workflow, validate_workflow, AgentExpressionPropertyName, Declaration, Expression, Workflow};
use superwire_core::semantic::support::expression::{evaluate_expression, EvaluationContext};
use superwire_core::semantic::support::provider::ProviderConfig;
use superwire_core::semantic::support::types::{validate_value_against_type, value_kind_name, workflow_type_to_json_schema, WorkflowType};
use superwire_core::semantic::{build_dynamic_typed_workflow_ir, build_execution_plan, ExecutionPlan, PlannedAgent, WorkflowSemanticError};
use tokio::sync::mpsc;

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

#[derive(Debug, Clone)]
pub struct WorkflowExecutor {
    workflow: Workflow,
    execution_plan: ExecutionPlan,
}

impl WorkflowExecutor {
    pub fn from_source(workflow_source: &str) -> Result<Self, ExecutorError> {
        let workflow = parse_workflow(workflow_source).map_err(|parse_error| {
            let details = parse_error.render_with_source(workflow_source, "<workflow>");

            WorkflowSemanticError::ParseFailed {
                source: parse_error,
                details,
            }
        })?;
        let validation_report = validate_workflow(&workflow);

        if validation_report.has_issues() {
            let issues = validation_report.render_with_source(workflow_source, "<workflow>");

            return Err(WorkflowSemanticError::InvalidWorkflow { issues }.into());
        }

        let typed_workflow_ir =
            build_dynamic_typed_workflow_ir(&workflow).map_err(|error| error.into_compilation_diagnostic(&workflow, "<workflow>"))?;
        let execution_plan = build_execution_plan(&workflow, &typed_workflow_ir)
            .map_err(|error| error.into_compilation_diagnostic(&workflow, "<workflow>"))?;

        Ok(Self { workflow, execution_plan })
    }

    #[must_use]
    pub fn agent_execution_order(&self) -> Vec<String> {
        self.execution_plan.agent_execution_order.clone()
    }

    pub async fn execute<ModelProviderType>(
        &self,
        input: Value,
        secrets: Value,
        model_provider: &ModelProviderType,
        event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    ) -> Result<Value, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        let input_values = self.resolve_input_values(&input)?;
        let secret_values = self.resolve_secret_values(&secrets)?;
        let mut runtime_state = RuntimeState::new(input_values, secret_values);

        self.execute_workflow_dynamic_blocks(&mut runtime_state)?;

        for execution_batch in self.resolve_agent_execution_batches()? {
            let runtime_state_snapshot = runtime_state.clone();
            let mut pending_executions = Vec::new();

            for agent_name in execution_batch {
                let planned_agent = self
                    .execution_plan
                    .planned_agents
                    .get(&agent_name)
                    .expect("planned agent should exist")
                    .clone();
                let runtime_state_snapshot = runtime_state_snapshot.clone();
                let event_sender = event_sender.clone();

                pending_executions.push(async move {
                    self.execute_agent(&planned_agent, &runtime_state_snapshot, model_provider, event_sender)
                        .await
                });
            }

            let completed_executions = try_join_all(pending_executions).await?;

            for completed_execution in completed_executions {
                completed_execution.apply_to_runtime_state(&mut runtime_state);
            }
        }

        let output = self.evaluate_workflow_output(&runtime_state)?;

        validate_value_against_type(&output, &self.execution_plan.workflow_output_type).map_err(|message| {
            ExecutorError::OutputTypeMismatch {
                expected: self.execution_plan.workflow_output_type.to_string(),
                found: format!("invalid runtime output: {message}"),
            }
        })?;

        Ok(output)
    }

    fn resolve_input_values(&self, input: &Value) -> Result<Map<String, Value>, ExecutorError> {
        if let Some(input_type) = &self.execution_plan.input_type {
            validate_value_against_type(input, input_type).map_err(|message| ExecutorError::InputValueMismatch { message })?;

            return input.as_object().cloned().ok_or_else(|| ExecutorError::InputValueMismatch {
                message: format!("expected input object, found {}", value_kind_name(input)),
            });
        }

        if input.is_null() || input.as_object().is_some_and(Map::is_empty) {
            return Ok(Map::new());
        }

        Err(ExecutorError::InputTypeMismatch {
            expected: "no input".to_string(),
            found: value_kind_name(input).to_string(),
        })
    }

    fn resolve_secret_values(&self, secrets: &Value) -> Result<Map<String, Value>, ExecutorError> {
        if let Some(secrets_type) = &self.execution_plan.secrets_type {
            validate_value_against_type(secrets, secrets_type).map_err(|message| ExecutorError::InputValueMismatch { message })?;

            return secrets.as_object().cloned().ok_or_else(|| ExecutorError::InputValueMismatch {
                message: format!("expected secrets object, found {}", value_kind_name(secrets)),
            });
        }

        if secrets.is_null() || secrets.as_object().is_some_and(Map::is_empty) {
            return Ok(Map::new());
        }

        Err(ExecutorError::InputTypeMismatch {
            expected: "no secrets".to_string(),
            found: value_kind_name(secrets).to_string(),
        })
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

    fn execute_workflow_dynamic_blocks(&self, runtime_state: &mut RuntimeState) -> Result<(), ExecutorError> {
        for declaration in self.workflow.declarations() {
            let Declaration::Dynamic(dynamic_block) = declaration else {
                continue;
            };

            for dynamic_field in &dynamic_block.fields {
                let field_value = evaluate_expression(
                    &dynamic_field.value,
                    &runtime_state.evaluation_context(HashMap::new()),
                    &format!("dynamic field `{}`", dynamic_field.name),
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
        event_sender: Option<mpsc::Sender<ExecutorEvent>>,
    ) -> Result<CompletedAgentExecution, ExecutorError>
    where
        ModelProviderType: ModelProvider,
    {
        if planned_agent.declaration.for_loop.is_some() {
            return Err(ExecutorError::Other {
                message: "for-loop agent execution is not implemented in the executor crate yet".to_string(),
            });
        }

        let evaluation_context = runtime_state.evaluation_context(HashMap::new());
        let provider_template = self
            .execution_plan
            .provider_index
            .get(&planned_agent.provider_name)
            .ok_or_else(|| ExecutorError::Other {
                message: format!("provider `{}` is not declared", planned_agent.provider_name),
            })?;
        let provider_config = provider_template.resolve(&planned_agent.provider_name, &evaluation_context)?;
        let ProviderConfig::OpenAI(openai_provider_config) = provider_config else {
            return Err(ExecutorError::Other {
                message: "executor only supports provider driver `openai`".to_string(),
            });
        };
        let model_name = evaluate_agent_model_name(&planned_agent.model_expression, &planned_agent.name, &evaluation_context)?;
        let prompt_expression = planned_agent
            .declaration
            .required_expression_property(AgentExpressionPropertyName::Prompt)
            .map_err(|missing_property| WorkflowSemanticError::InvalidAgentProperty {
                agent_name: planned_agent.name.clone(),
                property: missing_property.as_str().to_string(),
                message: "property is required".to_string(),
            })?;
        let prompt = normalize_prompt(evaluate_expression(
            prompt_expression,
            &evaluation_context,
            &format!("prompt for agent `{}`", planned_agent.name),
        )?);
        let output_schema = workflow_type_to_json_schema(&planned_agent.iteration_output_type);

        if let Some(event_sender) = &event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_started(
                    planned_agent.name.clone(),
                    model_name.clone(),
                    Vec::new(),
                ))
                .await;
        }

        let model_response = model_provider
            .generate(ModelRequest {
                agent_name: planned_agent.name.clone(),
                provider_config: openai_provider_config,
                model_name,
                prompt,
                output_schema,
            })
            .await?;

        validate_agent_output_value(&model_response.output, &planned_agent.iteration_output_type, &planned_agent.name)?;

        if let Some(event_sender) = &event_sender {
            let _ = event_sender
                .send(ExecutorEvent::agent_completed(
                    planned_agent.name.clone(),
                    model_response.output.clone(),
                ))
                .await;
        }

        Ok(CompletedAgentExecution {
            agent_name: planned_agent.name.clone(),
            output: model_response.output,
            context: model_response.context,
        })
    }

    fn evaluate_workflow_output(&self, runtime_state: &RuntimeState) -> Result<Value, ExecutorError> {
        let mut output_fields = Map::new();
        let evaluation_context = runtime_state.evaluation_context(HashMap::new());

        for output_field in &self.execution_plan.output_declaration.fields {
            let output_value = evaluate_expression(&output_field.value, &evaluation_context, "workflow output")?;
            output_fields.insert(output_field.name.clone(), output_value);
        }

        Ok(Value::Object(output_fields))
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

fn normalize_prompt(prompt_value: Value) -> String {
    if let Some(prompt) = prompt_value.as_str() {
        return prompt.to_string();
    }

    serde_json::to_string(&prompt_value).unwrap_or_else(|_| prompt_value.to_string())
}

fn validate_agent_output_value(output: &Value, expected_type: &WorkflowType, agent_name: &str) -> Result<(), ExecutorError> {
    validate_value_against_type(output, expected_type).map_err(|message| ExecutorError::AgentOutputTypeMismatch {
        agent_name: agent_name.to_string(),
        message,
    })
}
