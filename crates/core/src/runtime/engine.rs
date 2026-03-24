use crate::dsl::{parse_workflow, validate_workflow, AgentDeclaration, Declaration, Workflow};
use crate::runtime::error::{ValidationProblem, WorkflowRuntimeError};
use crate::runtime::evaluation::{
    build_finalize_parameters_schema_for_output_type, evaluate_expression, evaluate_provider_settings, evaluate_workflow_output,
    extract_model_binding, find_output_type_expression, find_prompt_expression, render_value_as_text,
    validate_value_against_type_expression,
};
use crate::runtime::graph::determine_agent_execution_order;
use crate::runtime::provider::{DynamicProvider, WorkflowProviderFactory};
use crate::runtime::types::{ExecutionScope, WorkflowExecutionResult};
use engine_ai_agent::{Agent, AgentConfig, Context, LoopExecutor};
use serde_json::Value;
use std::collections::HashMap;

pub struct WorkflowRuntime<FactoryType>
where
    FactoryType: WorkflowProviderFactory,
{
    provider_factory: FactoryType,
    agent_config: AgentConfig,
}

impl<FactoryType> WorkflowRuntime<FactoryType>
where
    FactoryType: WorkflowProviderFactory,
{
    #[must_use]
    pub fn new(provider_factory: FactoryType) -> Self {
        Self {
            provider_factory,
            agent_config: AgentConfig::default(),
        }
    }

    #[must_use]
    pub fn with_agent_config(mut self, agent_config: AgentConfig) -> Self {
        self.agent_config = agent_config;
        self
    }

    pub async fn execute_source(
        &self,
        workflow_source: &str,
        input_values: Value,
        secret_values: Value,
    ) -> Result<WorkflowExecutionResult, WorkflowRuntimeError> {
        let workflow = parse_workflow(workflow_source)?;

        self.execute_workflow(&workflow, input_values, secret_values).await
    }

    pub async fn execute_workflow(
        &self,
        workflow: &Workflow,
        input_values: Value,
        secret_values: Value,
    ) -> Result<WorkflowExecutionResult, WorkflowRuntimeError> {
        let validation_report = validate_workflow(workflow);

        if validation_report.has_issues() {
            let problems = validation_report
                .issues_with_spans()
                .map(|(issue, span)| ValidationProblem {
                    issue: issue.clone(),
                    span,
                })
                .collect::<Vec<_>>();

            return Err(WorkflowRuntimeError::ValidationFailed { problems });
        }

        let execution_order = determine_agent_execution_order(workflow)?;

        let mut agent_declarations_by_name = HashMap::<String, &AgentDeclaration>::new();

        for declaration in workflow.declarations() {
            if let Declaration::Agent(agent_declaration) = declaration {
                agent_declarations_by_name.insert(agent_declaration.name.clone(), agent_declaration);
            }
        }

        let mut agent_outputs_by_name = HashMap::<String, Value>::new();
        let mut agent_contexts_by_name = HashMap::<String, Context>::new();

        for agent_name in execution_order {
            let agent_declaration = agent_declarations_by_name
                .get(agent_name.as_str())
                .copied()
                .expect("execution order should include declared agents only");

            if agent_declaration.for_loop.is_some() {
                return Err(WorkflowRuntimeError::UnsupportedForLoop {
                    agent_name: agent_name.clone(),
                });
            }

            let model_binding = extract_model_binding(agent_declaration)?;

            let provider_declaration = workflow.find_provider(model_binding.provider_name.as_str()).ok_or_else(|| {
                WorkflowRuntimeError::MissingProviderDeclaration {
                    agent_name: agent_name.clone(),
                    provider_name: model_binding.provider_name.clone(),
                }
            })?;

            let execution_scope = ExecutionScope {
                input_values: &input_values,
                secret_values: &secret_values,
                agent_outputs_by_name: &agent_outputs_by_name,
            };

            let provider_settings = evaluate_provider_settings(
                provider_declaration,
                &execution_scope,
                format!("provider '{}'", provider_declaration.name).as_str(),
            )?;

            let provider = self.provider_factory.build_provider(
                agent_name.as_str(),
                provider_declaration.name.as_str(),
                &provider_settings,
                model_binding.model_name.as_str(),
            )?;

            let prompt_text = if let Some(prompt_expression) = find_prompt_expression(agent_declaration.properties.as_slice()) {
                let prompt_context = format!("agent '{agent_name}' prompt");
                let prompt_value = evaluate_expression(prompt_expression, &execution_scope, prompt_context.as_str())?;

                render_value_as_text(&prompt_value)
            } else {
                String::new()
            };

            let output_type_expression = find_output_type_expression(agent_declaration.properties.as_slice());

            let mut loop_executor =
                LoopExecutor::<DynamicProvider, Value>::new().map_err(|error| WorkflowRuntimeError::LoopExecutorCreationFailed {
                    agent_name: agent_name.clone(),
                    message: error.to_string(),
                })?;

            if let Some(output_type_expression) = output_type_expression {
                let finalize_parameters_schema = build_finalize_parameters_schema_for_output_type(output_type_expression, workflow)
                    .map_err(|message| WorkflowRuntimeError::LoopExecutorCreationFailed {
                        agent_name: agent_name.clone(),
                        message,
                    })?;

                loop_executor = loop_executor.with_finalize_parameters_schema(finalize_parameters_schema);
            }

            let run_result = Agent::new(loop_executor, provider)
                .with_config(self.agent_config.clone())
                .run(prompt_text)
                .await
                .map_err(|error| WorkflowRuntimeError::AgentExecutionFailed {
                    agent_name: agent_name.clone(),
                    message: error.to_string(),
                })?;

            if let Some(output_type_expression) = output_type_expression {
                let type_validation_result =
                    validate_value_against_type_expression(&run_result.output, output_type_expression, workflow, "$output");

                if let Err(validation_message) = type_validation_result {
                    return Err(WorkflowRuntimeError::AgentOutputTypeMismatch {
                        agent_name: agent_name.clone(),
                        message: validation_message,
                    });
                }
            }

            agent_outputs_by_name.insert(agent_name.clone(), run_result.output);
            agent_contexts_by_name.insert(agent_name, run_result.context);
        }

        let output = evaluate_workflow_output(workflow, &input_values, &secret_values, &agent_outputs_by_name)?;

        Ok(WorkflowExecutionResult {
            output,
            agent_outputs_by_name,
            agent_contexts_by_name,
        })
    }
}
