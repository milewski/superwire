use crate::dsl::{AgentDeclaration, AgentProperty, Expression, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::{evaluate_expression, EvaluationContext};
use crate::runtime::inference::InferenceSetting;
use crate::runtime::runner::{AgentExecutionRequest, AgentRunner, LoopAgentRunner};
use crate::runtime::types::{validate_value_against_type, value_kind_name, workflow_type_to_schemars_schema, WorkflowType};
use crate::semantic::{compile_workflow_pipeline, ExecutionPlan, PlannedAgent, WorkflowPipelineInput};
use engine_ai_agent::AgentConfig;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::marker::PhantomData;

#[derive(Debug, Clone)]
struct RuntimeState {
    input_values: Map<String, Value>,
    secret_values: Map<String, Value>,
    agent_outputs: HashMap<String, Value>,
    agent_contexts: HashMap<String, Value>,
}

impl RuntimeState {
    #[must_use]
    fn new(input_values: Map<String, Value>) -> Self {
        Self {
            input_values,
            secret_values: Map::new(),
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct CompiledWorkflow {
    execution_plan: ExecutionPlan,
}

pub struct WorkflowRuntime<Input, Output>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    workflow: Workflow,
    compiled_workflow: CompiledWorkflow,
    phantom: PhantomData<(Input, Output)>,
}

impl<Input, Output> WorkflowRuntime<Input, Output>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    pub fn new(workflow: Workflow) -> Result<Self, WorkflowRuntimeError> {
        let compiled_workflow = compile_workflow::<Input, Output>(&workflow)?;

        Ok(Self {
            workflow,
            compiled_workflow,
            phantom: PhantomData,
        })
    }

    #[must_use]
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    pub async fn run(&self, input: Input) -> Result<Output, WorkflowRuntimeError> {
        self.run_with_runner(input, &LoopAgentRunner).await
    }

    pub async fn run_with_runner<RunnerType>(&self, input: Input, runner: &RunnerType) -> Result<Output, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let serialized_input = serde_json::to_value(input).map_err(|source| WorkflowRuntimeError::SerializationFailed {
            context: "workflow input".to_string(),
            source,
        })?;

        let input_values = self.resolve_input_values(&serialized_input)?;

        let mut runtime_state = RuntimeState::new(input_values);
        let execution_order = self.resolve_agent_execution_order();

        for agent_name in execution_order {
            let planned_agent = self
                .compiled_workflow
                .execution_plan
                .planned_agents
                .get(&agent_name)
                .expect("agent should exist in execution plan")
                .clone();

            self.execute_agent(&planned_agent, &mut runtime_state, runner).await?;
        }

        let workflow_output_value = self.evaluate_workflow_output(&runtime_state)?;

        validate_value_against_type(&workflow_output_value, &self.compiled_workflow.execution_plan.workflow_output_type).map_err(
            |message| WorkflowRuntimeError::OutputTypeMismatch {
                expected: self.compiled_workflow.execution_plan.workflow_output_type.to_string(),
                found: format!("invalid runtime output: {message}"),
            },
        )?;

        serde_json::from_value::<Output>(workflow_output_value)
            .map_err(|source| WorkflowRuntimeError::OutputDeserializationFailed { source })
    }

    fn resolve_input_values(&self, serialized_input: &Value) -> Result<Map<String, Value>, WorkflowRuntimeError> {
        if let Some(input_type) = &self.compiled_workflow.execution_plan.input_type {
            validate_value_against_type(serialized_input, input_type)
                .map_err(|message| WorkflowRuntimeError::InputValueMismatch { message })?;

            let Some(input_values) = serialized_input.as_object() else {
                return Err(WorkflowRuntimeError::InputValueMismatch {
                    message: format!("expected input object, found {}", value_kind_name(serialized_input)),
                });
            };

            Ok(input_values.clone())
        } else {
            if serialized_input.is_null() {
                return Ok(Map::new());
            }

            if let Some(input_values) = serialized_input.as_object() {
                if input_values.is_empty() {
                    return Ok(Map::new());
                }
            }

            Err(WorkflowRuntimeError::InputTypeMismatch {
                expected: "no input".to_string(),
                found: value_kind_name(serialized_input).to_string(),
            })
        }
    }

    fn resolve_agent_execution_order(&self) -> Vec<String> {
        self.compiled_workflow.execution_plan.agent_execution_order.clone()
    }

    async fn execute_agent<RunnerType>(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &mut RuntimeState,
        runner: &RunnerType,
    ) -> Result<(), WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let agent_name = planned_agent.name.clone();
        let agent_declaration = &planned_agent.declaration;

        let Some(provider_config) = self
            .compiled_workflow
            .execution_plan
            .provider_index
            .get(&planned_agent.provider_name)
        else {
            return Err(WorkflowRuntimeError::ProviderConfiguration {
                provider_name: planned_agent.provider_name.clone(),
                message: "provider referenced by execution plan is not declared".to_string(),
            });
        };

        let agent_prompt_expression = required_agent_property_expression(agent_declaration, "prompt")?;
        let context_property_expression = optional_agent_property_expression(agent_declaration, "context");

        if optional_agent_property_expression(agent_declaration, "tools").is_some() {
            return Err(WorkflowRuntimeError::UnsupportedFeature {
                feature: format!("agent `{agent_name}` uses `tools`, which is not supported yet"),
            });
        }

        let base_config = build_agent_config(agent_declaration, runtime_state)?;
        let iteration_output_type = planned_agent.iteration_output_type.clone();
        let iteration_output_schema = workflow_type_to_schemars_schema(&iteration_output_type)?;

        if let Some(for_loop) = &agent_declaration.for_loop {
            let iterable_value = evaluate_expression(
                &for_loop.iterable,
                &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
                &format!("for-loop iterable for agent `{agent_name}`"),
            )?;

            let Some(iterable_items) = iterable_value.as_array() else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: format!("for-loop iterable for agent `{agent_name}`"),
                    message: format!("expected array iterable, found {}", value_kind_name(&iterable_value)),
                });
            };

            let mut iteration_outputs = Vec::new();
            let mut iteration_contexts = Vec::new();

            for iterable_item in iterable_items {
                let mut local_bindings = HashMap::new();
                local_bindings.insert(for_loop.iterator_name.clone(), iterable_item.clone());

                let prompt_value = evaluate_expression(
                    agent_prompt_expression,
                    &runtime_state_to_evaluation_context(runtime_state, local_bindings.clone()),
                    &format!("prompt for agent `{agent_name}`"),
                )?;

                let prompt = normalize_prompt(prompt_value);
                let prompt =
                    apply_optional_context_prefix(prompt, context_property_expression, runtime_state, local_bindings, &agent_name)?;

                let request = AgentExecutionRequest {
                    agent_name: agent_name.clone(),
                    provider_config: provider_config.clone(),
                    model_name: planned_agent.model_name.clone(),
                    prompt,
                    config: base_config.clone(),
                    output_schema: iteration_output_schema.clone(),
                };

                let agent_result = runner.run_agent(&request).await?;
                validate_agent_output_value(&agent_result.output, &iteration_output_type, &agent_name)?;

                iteration_outputs.push(agent_result.output);
                iteration_contexts.push(agent_result.context);
            }

            runtime_state
                .agent_outputs
                .insert(agent_name.clone(), Value::Array(iteration_outputs));

            runtime_state
                .agent_contexts
                .insert(agent_name.clone(), Value::Array(iteration_contexts));

            return Ok(());
        }

        let prompt_value = evaluate_expression(
            agent_prompt_expression,
            &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
            &format!("prompt for agent `{agent_name}`"),
        )?;

        let prompt = normalize_prompt(prompt_value);
        let prompt = apply_optional_context_prefix(prompt, context_property_expression, runtime_state, HashMap::new(), &agent_name)?;

        let request = AgentExecutionRequest {
            agent_name: agent_name.clone(),
            provider_config: provider_config.clone(),
            model_name: planned_agent.model_name.clone(),
            prompt,
            config: base_config,
            output_schema: iteration_output_schema,
        };

        let agent_result = runner.run_agent(&request).await?;
        validate_agent_output_value(&agent_result.output, &iteration_output_type, &agent_name)?;

        runtime_state.agent_outputs.insert(agent_name.clone(), agent_result.output);

        runtime_state.agent_contexts.insert(agent_name, agent_result.context);

        Ok(())
    }

    fn evaluate_workflow_output(&self, runtime_state: &RuntimeState) -> Result<Value, WorkflowRuntimeError> {
        let mut output_fields = Map::new();
        let evaluation_context = runtime_state_to_evaluation_context(runtime_state, HashMap::new());

        for output_field in &self.compiled_workflow.execution_plan.output_declaration.fields {
            let output_value = evaluate_expression(&output_field.value, &evaluation_context, "workflow output")?;
            output_fields.insert(output_field.name.clone(), output_value);
        }

        Ok(Value::Object(output_fields))
    }
}

pub async fn execute_workflow<Input, Output>(workflow: &Workflow, input: Input) -> Result<Output, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    WorkflowRuntime::<Input, Output>::new(workflow.clone())?.run(input).await
}

pub async fn execute_workflow_without_input<Output>(workflow: &Workflow) -> Result<Output, WorkflowRuntimeError>
where
    Output: DeserializeOwned + JsonSchema,
{
    execute_workflow(workflow, ()).await
}

fn compile_workflow<Input, Output>(workflow: &Workflow) -> Result<CompiledWorkflow, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    let plan_stage_output = compile_workflow_pipeline::<Input, Output>(WorkflowPipelineInput::Workflow(workflow))?;

    Ok(CompiledWorkflow {
        execution_plan: plan_stage_output.into_execution_plan(),
    })
}

fn required_agent_property_expression<'property>(
    agent_declaration: &'property AgentDeclaration,
    property_name: &str,
) -> Result<&'property Expression, WorkflowRuntimeError> {
    optional_agent_property_expression(agent_declaration, property_name).ok_or_else(|| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_declaration.name.clone(),
        property: property_name.to_string(),
        message: "property is required".to_string(),
    })
}

fn optional_agent_property_expression<'property>(
    agent_declaration: &'property AgentDeclaration,
    property_name: &str,
) -> Option<&'property Expression> {
    for agent_property in &agent_declaration.properties {
        match agent_property {
            AgentProperty::Model(expression) if property_name == "model" => return Some(expression),
            AgentProperty::Prompt(expression) if property_name == "prompt" => return Some(expression),
            AgentProperty::Context(expression) if property_name == "context" => return Some(expression),
            AgentProperty::Inference(expression) if property_name == "inference" => return Some(expression),
            AgentProperty::Tools(expression) if property_name == "tools" => return Some(expression),
            AgentProperty::Custom { name, value } if name == property_name => return Some(value),
            AgentProperty::Model(_)
            | AgentProperty::Prompt(_)
            | AgentProperty::Output(_)
            | AgentProperty::Context(_)
            | AgentProperty::Inference(_)
            | AgentProperty::Tools(_)
            | AgentProperty::Custom { name: _, value: _ } => {}
        }
    }

    None
}

fn build_agent_config(agent_declaration: &AgentDeclaration, runtime_state: &RuntimeState) -> Result<AgentConfig, WorkflowRuntimeError> {
    let Some(inference_expression) = optional_agent_property_expression(agent_declaration, "inference") else {
        return Ok(AgentConfig::default());
    };

    let inference_value = evaluate_expression(
        inference_expression,
        &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
        &format!("inference for agent `{}`", agent_declaration.name),
    )?;

    let Some(inference_fields) = inference_value.as_object() else {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: "inference".to_string(),
            message: "inference must evaluate to an object".to_string(),
        });
    };

    let mut config = AgentConfig::default();

    for inference_setting in InferenceSetting::all() {
        config = inference_setting.apply(config, inference_fields, &agent_declaration.name)?;
    }

    Ok(config)
}

fn validate_agent_output_value(
    output_value: &Value,
    expected_output_type: &WorkflowType,
    agent_name: &str,
) -> Result<(), WorkflowRuntimeError> {
    validate_value_against_type(output_value, expected_output_type).map_err(|message| WorkflowRuntimeError::AgentOutputTypeMismatch {
        agent_name: agent_name.to_string(),
        message,
    })
}

fn normalize_prompt(prompt_value: Value) -> String {
    if let Some(prompt) = prompt_value.as_str() {
        return prompt.to_string();
    }

    serde_json::to_string(&prompt_value).unwrap_or_else(|_| prompt_value.to_string())
}

fn apply_optional_context_prefix(
    prompt: String,
    context_expression: Option<&Expression>,
    runtime_state: &RuntimeState,
    local_bindings: HashMap<String, Value>,
    agent_name: &str,
) -> Result<String, WorkflowRuntimeError> {
    let Some(context_expression) = context_expression else {
        return Ok(prompt);
    };

    let context_value = evaluate_expression(
        context_expression,
        &runtime_state_to_evaluation_context(runtime_state, local_bindings),
        &format!("context for agent `{agent_name}`"),
    )?;

    let context_text = serde_json::to_string_pretty(&context_value).unwrap_or_else(|_| context_value.to_string());

    Ok(format!("Context:\n{context_text}\n\nTask:\n{prompt}"))
}

fn runtime_state_to_evaluation_context(runtime_state: &RuntimeState, local_bindings: HashMap<String, Value>) -> EvaluationContext {
    EvaluationContext {
        input_values: runtime_state.input_values.clone(),
        secret_values: runtime_state.secret_values.clone(),
        agent_outputs: runtime_state.agent_outputs.clone(),
        agent_contexts: runtime_state.agent_contexts.clone(),
        local_bindings,
    }
}
