use crate::dsl::{
    AgentDeclaration, AgentExpressionPropertyName, AgentForLoop, CallArgument, Expression, FunctionCall, Reference, ReferenceKeyword,
    Workflow,
};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::{evaluate_expression, EvaluationContext};
use crate::runtime::inference::InferenceSetting;
use crate::runtime::provider::ProviderConfig;
use crate::runtime::runner::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, LoopAgentRunner, RequestedAgentTool};
use crate::runtime::types::{validate_value_against_type, value_kind_name, workflow_type_to_schemars_schema, WorkflowType};
use crate::semantic::{compile_workflow_pipeline, ExecutionPlan, PlannedAgent, WorkflowPipelineInput};
use engine_ai_agent::AgentConfig;
use futures::future::try_join_all;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
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
    fn new(input_values: Map<String, Value>, secret_values: Map<String, Value>) -> Self {
        Self {
            input_values,
            secret_values,
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct CompiledWorkflow {
    execution_plan: ExecutionPlan,
}

#[derive(Debug, Clone)]
struct PreparedAgentExecution<'workflow> {
    agent_name: String,
    provider_config: ProviderConfig,
    model_expression: &'workflow Expression,
    prompt_expression: &'workflow Expression,
    context_expression: Option<&'workflow Expression>,
    output_type: WorkflowType,
    output_schema: Schema,
    config: AgentConfig,
    tools: Vec<AgentToolBinding>,
}

#[derive(Debug, Clone)]
struct AgentToolBinding {
    tool_name: String,
    argument_expressions: Vec<AgentToolArgumentExpression>,
}

#[derive(Debug, Clone)]
struct AgentToolArgumentExpression {
    argument_name: String,
    expression: Expression,
}

#[derive(Debug, Clone)]
struct CompletedAgentExecution {
    agent_name: String,
    output: Value,
    context: Value,
}

impl CompletedAgentExecution {
    fn apply_to_runtime_state(self, runtime_state: &mut RuntimeState) {
        runtime_state.agent_outputs.insert(self.agent_name.clone(), self.output);
        runtime_state.agent_contexts.insert(self.agent_name, self.context);
    }
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
        self.run_with_runner_and_secrets(input, (), &LoopAgentRunner).await
    }

    pub async fn run_with_runner<RunnerType>(&self, input: Input, runner: &RunnerType) -> Result<Output, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        self.run_with_runner_and_secrets(input, (), runner).await
    }

    pub async fn run_with_secrets<Secrets>(&self, input: Input, secrets: Secrets) -> Result<Output, WorkflowRuntimeError>
    where
        Secrets: Serialize,
    {
        self.run_with_runner_and_secrets(input, secrets, &LoopAgentRunner).await
    }

    pub async fn run_with_runner_and_secrets<RunnerType, Secrets>(
        &self,
        input: Input,
        secrets: Secrets,
        runner: &RunnerType,
    ) -> Result<Output, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
        Secrets: Serialize,
    {
        let serialized_input = serde_json::to_value(input).map_err(|source| WorkflowRuntimeError::SerializationFailed {
            context: "workflow input".to_string(),
            source,
        })?;

        let serialized_secrets = serde_json::to_value(secrets).map_err(|source| WorkflowRuntimeError::SerializationFailed {
            context: "workflow secrets".to_string(),
            source,
        })?;

        let input_values = self.resolve_input_values(&serialized_input)?;
        let secret_values = self.resolve_secret_values(&serialized_secrets)?;

        let mut runtime_state = RuntimeState::new(input_values, secret_values);
        let execution_order = self.resolve_agent_execution_order();
        let execution_batches = self.resolve_agent_execution_batches(&execution_order)?;

        for execution_batch in execution_batches {
            let runtime_state_snapshot = runtime_state.clone();
            let mut pending_executions = Vec::new();

            for agent_name in execution_batch {
                let planned_agent = self
                    .compiled_workflow
                    .execution_plan
                    .planned_agents
                    .get(&agent_name)
                    .expect("agent should exist in execution plan")
                    .clone();

                let runtime_state_snapshot = runtime_state_snapshot.clone();

                let execution_future = async move { self.execute_agent(&planned_agent, &runtime_state_snapshot, runner).await };

                pending_executions.push(execution_future);
            }

            let completed_executions = try_join_all(pending_executions).await?;

            for completed_execution in completed_executions {
                completed_execution.apply_to_runtime_state(&mut runtime_state);
            }
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

    fn resolve_secret_values(&self, serialized_secrets: &Value) -> Result<Map<String, Value>, WorkflowRuntimeError> {
        let Some(secrets_type) = &self.compiled_workflow.execution_plan.secrets_type else {
            if serialized_secrets.is_null() {
                return Ok(Map::new());
            }

            if let Some(secret_values) = serialized_secrets.as_object() {
                if secret_values.is_empty() {
                    return Ok(Map::new());
                }
            }

            return Err(WorkflowRuntimeError::InputTypeMismatch {
                expected: "no secrets".to_string(),
                found: value_kind_name(serialized_secrets).to_string(),
            });
        };

        validate_value_against_type(serialized_secrets, secrets_type)
            .map_err(|message| WorkflowRuntimeError::InputValueMismatch { message })?;

        let Some(secret_values) = serialized_secrets.as_object() else {
            return Err(WorkflowRuntimeError::InputValueMismatch {
                message: format!("expected secrets object, found {}", value_kind_name(serialized_secrets)),
            });
        };

        Ok(secret_values.clone())
    }

    fn resolve_agent_execution_order(&self) -> Vec<String> {
        self.compiled_workflow.execution_plan.agent_execution_order.clone()
    }

    fn resolve_agent_execution_batches(&self, execution_order: &[String]) -> Result<Vec<Vec<String>>, WorkflowRuntimeError> {
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
                    .compiled_workflow
                    .execution_plan
                    .planned_agents
                    .get(agent_name)
                    .expect("agent should exist in execution plan");

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
                let mut blocked_agent_names = unresolved_agents.into_iter().collect::<Vec<_>>();
                blocked_agent_names.sort();

                return Err(WorkflowRuntimeError::ExecutionPlanInvariant {
                    message: format!(
                        "failed to resolve execution batches; blocked agents: {}",
                        blocked_agent_names.join(", ")
                    ),
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

    async fn execute_agent<RunnerType>(
        &self,
        planned_agent: &PlannedAgent,
        runtime_state: &RuntimeState,
        runner: &RunnerType,
    ) -> Result<CompletedAgentExecution, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let prepared_agent_execution = self.prepare_agent_execution(planned_agent, runtime_state)?;

        if let Some(agent_for_loop) = &planned_agent.declaration.for_loop {
            return self
                .execute_for_loop_agent(agent_for_loop, &prepared_agent_execution, runtime_state, runner)
                .await;
        }

        self.execute_single_agent(&prepared_agent_execution, runtime_state, runner).await
    }

    fn prepare_agent_execution<'workflow>(
        &self,
        planned_agent: &'workflow PlannedAgent,
        runtime_state: &RuntimeState,
    ) -> Result<PreparedAgentExecution<'workflow>, WorkflowRuntimeError> {
        let agent_declaration = &planned_agent.declaration;
        let agent_name = planned_agent.name.clone();

        let Some(provider_config_template) = self
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

        let provider_config = provider_config_template.resolve(
            &planned_agent.provider_name,
            &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
        )?;

        let prompt_expression = agent_declaration
            .required_expression_property(AgentExpressionPropertyName::Prompt)
            .map_err(|missing_property| WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: missing_property.as_str().to_string(),
                message: "property is required".to_string(),
            })?;

        let tools = if let Some(tools_expression) = agent_declaration.expression_property(AgentExpressionPropertyName::Tools) {
            tools_expression.parse_agent_tools_expression(agent_declaration)?
        } else {
            Vec::new()
        };

        let output_type = planned_agent.iteration_output_type.clone();
        let output_schema = workflow_type_to_schemars_schema(&output_type)?;
        let config = build_agent_config(agent_declaration, runtime_state)?;

        Ok(PreparedAgentExecution {
            agent_name,
            provider_config,
            model_expression: &planned_agent.model_expression,
            prompt_expression,
            context_expression: agent_declaration.expression_property(AgentExpressionPropertyName::Context),
            output_type,
            output_schema,
            config,
            tools,
        })
    }

    async fn execute_for_loop_agent<RunnerType>(
        &self,
        agent_for_loop: &AgentForLoop,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        runner: &RunnerType,
    ) -> Result<CompletedAgentExecution, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let iterable_value = evaluate_expression(
            &agent_for_loop.iterable,
            &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
            &format!("for-loop iterable for agent `{}`", prepared_agent_execution.agent_name),
        )?;

        let Some(iterable_items) = iterable_value.as_array() else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: format!("for-loop iterable for agent `{}`", prepared_agent_execution.agent_name),
                message: format!("expected array iterable, found {}", value_kind_name(&iterable_value)),
            });
        };

        let mut pending_iteration_executions = Vec::new();

        for iterable_item in iterable_items {
            let mut local_bindings = HashMap::new();
            local_bindings.insert(agent_for_loop.iterator_name.clone(), iterable_item.clone());

            let model_name = self.evaluate_agent_model_name(prepared_agent_execution, runtime_state, local_bindings.clone())?;

            let prompt = self.evaluate_agent_prompt(prepared_agent_execution, runtime_state, local_bindings.clone())?;
            let tools = self.evaluate_agent_tools(prepared_agent_execution, runtime_state, local_bindings)?;

            let pending_iteration_execution = async move {
                let agent_result = self
                    .run_agent_request(prepared_agent_execution, model_name, prompt, tools, runner)
                    .await?;

                validate_agent_output_value(
                    &agent_result.output,
                    &prepared_agent_execution.output_type,
                    &prepared_agent_execution.agent_name,
                )?;

                Ok::<AgentExecutionResult, WorkflowRuntimeError>(agent_result)
            };

            pending_iteration_executions.push(pending_iteration_execution);
        }

        let completed_iteration_executions = try_join_all(pending_iteration_executions).await?;
        let mut iteration_outputs = Vec::with_capacity(completed_iteration_executions.len());
        let mut iteration_contexts = Vec::with_capacity(completed_iteration_executions.len());

        for completed_iteration_execution in completed_iteration_executions {
            iteration_outputs.push(completed_iteration_execution.output);
            iteration_contexts.push(completed_iteration_execution.context);
        }

        Ok(CompletedAgentExecution {
            agent_name: prepared_agent_execution.agent_name.clone(),
            output: Value::Array(iteration_outputs),
            context: Value::Array(iteration_contexts),
        })
    }

    async fn execute_single_agent<RunnerType>(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        runner: &RunnerType,
    ) -> Result<CompletedAgentExecution, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let model_name = self.evaluate_agent_model_name(prepared_agent_execution, runtime_state, HashMap::new())?;
        let prompt = self.evaluate_agent_prompt(prepared_agent_execution, runtime_state, HashMap::new())?;
        let tools = self.evaluate_agent_tools(prepared_agent_execution, runtime_state, HashMap::new())?;
        let agent_result = self
            .run_agent_request(prepared_agent_execution, model_name, prompt, tools, runner)
            .await?;

        validate_agent_output_value(
            &agent_result.output,
            &prepared_agent_execution.output_type,
            &prepared_agent_execution.agent_name,
        )?;

        Ok(CompletedAgentExecution {
            agent_name: prepared_agent_execution.agent_name.clone(),
            output: agent_result.output,
            context: agent_result.context,
        })
    }

    fn evaluate_agent_tools(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
    ) -> Result<Vec<RequestedAgentTool>, WorkflowRuntimeError> {
        let mut resolved_tools = Vec::new();

        let evaluation_context = runtime_state_to_evaluation_context(runtime_state, local_bindings);

        for tool_binding in &prepared_agent_execution.tools {
            let mut resolved_bound_arguments = Map::new();

            for argument_expression in &tool_binding.argument_expressions {
                let argument_value = evaluate_expression(
                    &argument_expression.expression,
                    &evaluation_context,
                    &format!(
                        "tool `tool.{}` argument `{}` for agent `{}`",
                        tool_binding.tool_name, argument_expression.argument_name, prepared_agent_execution.agent_name
                    ),
                )?;

                resolved_bound_arguments.insert(argument_expression.argument_name.clone(), argument_value);
            }

            resolved_tools.push(RequestedAgentTool {
                name: tool_binding.tool_name.clone(),
                bound_arguments: resolved_bound_arguments,
            });
        }

        Ok(resolved_tools)
    }

    fn evaluate_agent_prompt(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
    ) -> Result<String, WorkflowRuntimeError> {
        let prompt_value = evaluate_expression(
            prepared_agent_execution.prompt_expression,
            &runtime_state_to_evaluation_context(runtime_state, local_bindings.clone()),
            &format!("prompt for agent `{}`", prepared_agent_execution.agent_name),
        )?;

        let prompt = normalize_prompt(prompt_value);

        apply_optional_context_prefix(
            prompt,
            prepared_agent_execution.context_expression,
            runtime_state,
            local_bindings,
            &prepared_agent_execution.agent_name,
        )
    }

    fn evaluate_agent_model_name(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
    ) -> Result<String, WorkflowRuntimeError> {
        prepared_agent_execution.model_expression.evaluate_as_agent_model_name(
            &prepared_agent_execution.agent_name,
            runtime_state,
            local_bindings,
        )
    }

    async fn run_agent_request<RunnerType>(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        model_name: String,
        prompt: String,
        tools: Vec<RequestedAgentTool>,
        runner: &RunnerType,
    ) -> Result<AgentExecutionResult, WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let request = AgentExecutionRequest {
            agent_name: prepared_agent_execution.agent_name.clone(),
            provider_config: prepared_agent_execution.provider_config.clone(),
            model_name,
            prompt,
            config: prepared_agent_execution.config.clone(),
            output_schema: prepared_agent_execution.output_schema.clone(),
            requested_tools: tools,
            runtime_tools: Vec::new(),
        };

        runner.run_agent(&request).await
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

impl Expression {
    fn evaluate_as_agent_model_name(
        &self,
        agent_name: &str,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
    ) -> Result<String, WorkflowRuntimeError> {
        let model_value = evaluate_expression(
            self,
            &runtime_state_to_evaluation_context(runtime_state, local_bindings),
            &format!("model for agent `{agent_name}`"),
        )?;

        let Some(model_name) = model_value.as_str() else {
            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_name.to_string(),
                property: AgentExpressionPropertyName::Model.as_str().to_string(),
                message: format!("model must resolve to a string, found {}", value_kind_name(&model_value)),
            });
        };

        Ok(model_name.to_string())
    }

    fn parse_agent_tools_expression(&self, agent_declaration: &AgentDeclaration) -> Result<Vec<AgentToolBinding>, WorkflowRuntimeError> {
        let Expression::ArrayLiteral(tool_expressions) = self else {
            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: "tools must be an array literal like [tool.weather, tool.weather(country: input.country)]".to_string(),
            });
        };

        let mut parsed_tools = Vec::new();

        for tool_expression in tool_expressions {
            parsed_tools.push(tool_expression.parse_agent_tool_binding(agent_declaration)?);
        }

        Ok(parsed_tools)
    }

    fn parse_agent_tool_binding(&self, agent_declaration: &AgentDeclaration) -> Result<AgentToolBinding, WorkflowRuntimeError> {
        match self {
            Self::Reference(reference) => {
                let tool_name = reference.parse_agent_tool_name(agent_declaration)?;

                Ok(AgentToolBinding {
                    tool_name,
                    argument_expressions: Vec::new(),
                })
            }
            Self::FunctionCall(function_call) => function_call.parse_agent_tool_call(agent_declaration),
            _ => Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: "each tools entry must be either `tool.name` or `tool.name(arg: expression, ...)`".to_string(),
            }),
        }
    }
}

impl Reference {
    fn parse_agent_tool_name(&self, agent_declaration: &AgentDeclaration) -> Result<String, WorkflowRuntimeError> {
        if self.root_keyword() != Some(ReferenceKeyword::Tool) {
            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: "tools entries must reference the `tool` namespace".to_string(),
            });
        }

        if self.accesses.len() != 1 || self.accesses[0].optional {
            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: "tools entries must use a direct tool reference like `tool.weather`".to_string(),
            });
        }

        Ok(self.accesses[0].field.clone())
    }
}

impl FunctionCall {
    fn parse_agent_tool_call(&self, agent_declaration: &AgentDeclaration) -> Result<AgentToolBinding, WorkflowRuntimeError> {
        let tool_name = self.callee.parse_agent_tool_name(agent_declaration)?;
        let mut argument_expressions = Vec::new();
        let mut seen_argument_names = std::collections::HashSet::<String>::new();

        for call_argument in &self.arguments {
            let CallArgument::Named(named_argument) = call_argument else {
                return Err(WorkflowRuntimeError::InvalidAgentProperty {
                    agent_name: agent_declaration.name.clone(),
                    property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                    message: format!(
                        "tool call `tool.{tool_name}(...)` only supports named arguments (for example `tool.{tool_name}(country: input.country)` )"
                    ),
                });
            };

            if !seen_argument_names.insert(named_argument.name.clone()) {
                return Err(WorkflowRuntimeError::InvalidAgentProperty {
                    agent_name: agent_declaration.name.clone(),
                    property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                    message: format!(
                        "tool call `tool.{tool_name}(...)` has duplicate named argument `{}`",
                        named_argument.name
                    ),
                });
            }

            argument_expressions.push(AgentToolArgumentExpression {
                argument_name: named_argument.name.clone(),
                expression: named_argument.value.clone(),
            });
        }

        Ok(AgentToolBinding {
            tool_name,
            argument_expressions,
        })
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

fn build_agent_config(agent_declaration: &AgentDeclaration, runtime_state: &RuntimeState) -> Result<AgentConfig, WorkflowRuntimeError> {
    let Some(inference_expression) = agent_declaration.expression_property(AgentExpressionPropertyName::Inference) else {
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
            property: AgentExpressionPropertyName::Inference.as_str().to_string(),
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
