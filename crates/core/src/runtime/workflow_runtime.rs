use crate::dsl::{
    parse_workflow, AgentDeclaration, AgentExpressionPropertyName, AgentForLoop, AgentForLoopPattern, AgentProperty, CallArgument,
    Declaration, Expression, FunctionCall, Reference, ReferenceKeyword, ToolCall, ToolDeclaration, TypeExpression, Workflow,
};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::{evaluate_expression, EvaluationContext};
use crate::runtime::inference::InferenceSetting;
use crate::runtime::provider::ProviderConfig;
use crate::runtime::runner::{AgentExecutionRequest, AgentExecutionResult, AgentRunner, LoopAgentRunner, RequestedAgentTool};
use crate::runtime::type_inference::TypeInferenceContext;
use crate::runtime::types::{
    ensure_type_matches, validate_value_against_type, value_kind_name, workflow_type_from_dsl, workflow_type_to_schemars_schema,
    WorkflowType,
};
use crate::semantic::{compile_workflow_pipeline, ExecutionPlan, PlannedAgent, WorkflowPipelineInput};
use futures::future::try_join_all;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;
use superwire_agent::tool::registered_runtime_tools;
use superwire_agent::AgentConfig;
use superwire_agent::DynamicTool;
use superwire_agent::RuntimeTool;
use superwire_agent::ToolDefinition;

#[derive(Debug, Clone)]
struct RuntimeState {
    input_values: Map<String, Value>,
    secret_values: Map<String, Value>,
    agent_outputs: HashMap<String, Value>,
    agent_contexts: HashMap<String, Value>,
    local_bindings: HashMap<String, Value>,
}

impl RuntimeState {
    #[must_use]
    fn new(input_values: Map<String, Value>, secret_values: Map<String, Value>) -> Self {
        Self {
            input_values,
            secret_values,
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
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
    agent_declaration: &'workflow AgentDeclaration,
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
    definition: Option<ToolDefinition>,
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

#[derive(Debug, Clone)]
struct RuntimeToolCatalog {
    tool_definitions: HashMap<String, ToolDefinition>,
    bound_argument_types: HashMap<String, HashMap<String, WorkflowType>>,
}

impl CompletedAgentExecution {
    fn apply_to_runtime_state(self, runtime_state: &mut RuntimeState) {
        runtime_state.agent_outputs.insert(self.agent_name.clone(), self.output);
        runtime_state.agent_contexts.insert(self.agent_name, self.context);
    }
}

impl ExecutionPlan {
    fn type_inference_context(&self) -> TypeInferenceContext {
        let agent_output_types = self
            .planned_agents
            .iter()
            .map(|(agent_name, planned_agent)| (agent_name.clone(), planned_agent.final_output_type.clone()))
            .collect::<HashMap<_, _>>();

        TypeInferenceContext {
            input_type: self.input_type.clone(),
            secrets_type: self.secrets_type.clone(),
            agent_output_types,
            tool_input_types: HashMap::new(),
            tool_binding_types: HashMap::new(),
            tool_output_types: HashMap::new(),
            local_binding_types: HashMap::new(),
        }
    }
}

trait WorkflowToolSchemaExt {
    fn named_schema_types(&self) -> HashMap<String, TypeExpression>;
}

impl WorkflowToolSchemaExt for Workflow {
    fn named_schema_types(&self) -> HashMap<String, TypeExpression> {
        self.declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                crate::dsl::Declaration::Schema(schema_declaration) => Some((
                    schema_declaration.name.clone(),
                    TypeExpression::Object(schema_declaration.fields.clone()),
                )),
                _ => None,
            })
            .collect()
    }
}

impl ToolDeclaration {
    fn to_tool_definition(&self, named_schema_types: &HashMap<String, TypeExpression>) -> Result<ToolDefinition, WorkflowRuntimeError> {
        let input_type = WorkflowType::from_tool_fields(self.input_fields.clone(), named_schema_types)?;
        let bounded_type = WorkflowType::from_tool_fields(self.binding_fields.clone(), named_schema_types)?;
        let parameters_schema = workflow_type_to_schemars_schema(&input_type, None)?;
        let bound_parameters_schema = if self.binding_fields.is_empty() {
            None
        } else {
            Some(workflow_type_to_schemars_schema(&bounded_type, None)?)
        };
        let output_type = WorkflowType::from_tool_fields(self.output_fields.clone(), named_schema_types)?;
        let output_schema = if self.output_fields.is_empty() {
            None
        } else {
            Some(workflow_type_to_schemars_schema(&output_type, None)?)
        };

        Ok(ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone().unwrap_or_default(),
            parameters_schema,
            bound_parameters_schema,
            output_schema,
        })
    }

    fn bound_argument_types(
        &self,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<HashMap<String, WorkflowType>, WorkflowRuntimeError> {
        let mut bound_argument_types = HashMap::new();

        for bounded_field in &self.binding_fields {
            let bound_argument_type = workflow_type_from_dsl(&bounded_field.field_type, named_schema_types)?;
            bound_argument_types.insert(bounded_field.name.clone(), bound_argument_type);
        }

        Ok(bound_argument_types)
    }
}

trait ToolFieldsWorkflowTypeExt {
    fn from_tool_fields(
        fields: Vec<crate::dsl::TypedField>,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Self, WorkflowRuntimeError>
    where
        Self: Sized;
}

impl ToolFieldsWorkflowTypeExt for WorkflowType {
    fn from_tool_fields(
        fields: Vec<crate::dsl::TypedField>,
        named_schema_types: &HashMap<String, TypeExpression>,
    ) -> Result<Self, WorkflowRuntimeError> {
        workflow_type_from_dsl(&TypeExpression::Object(fields), named_schema_types)
    }
}

impl RuntimeToolCatalog {
    fn from_workflow_and_runtime_tools(workflow: &Workflow, dynamic_runtime_tools: &[DynamicTool]) -> Result<Self, WorkflowRuntimeError> {
        let mut tool_definitions = HashMap::<String, ToolDefinition>::new();
        let mut bound_argument_types = HashMap::<String, HashMap<String, WorkflowType>>::new();

        let named_schema_types = workflow.named_schema_types();

        for declaration in workflow.declarations() {
            let crate::dsl::Declaration::Tool(tool_declaration) = declaration else {
                continue;
            };

            let tool_definition = tool_declaration.to_tool_definition(&named_schema_types)?;
            let tool_bound_argument_types = tool_declaration.bound_argument_types(&named_schema_types)?;

            if tool_definitions
                .insert(tool_definition.name.clone(), tool_definition.clone())
                .is_some()
            {
                return Err(WorkflowRuntimeError::Other {
                    message: format!("duplicate runtime tool name `{}` while building tool catalog", tool_definition.name),
                });
            }

            bound_argument_types.insert(tool_definition.name.clone(), tool_bound_argument_types);
        }

        for registered_tool in registered_runtime_tools() {
            let tool_definition = registered_tool.definition().map_err(|error| WorkflowRuntimeError::Other {
                message: format!("failed to read definition for registered runtime tool: {error}"),
            })?;

            tool_definitions.entry(tool_definition.name.clone()).or_insert(tool_definition);
        }

        for dynamic_runtime_tool in dynamic_runtime_tools {
            let tool_definition = dynamic_runtime_tool.tool_definition().clone();

            tool_definitions.entry(tool_definition.name.clone()).or_insert(tool_definition);
        }

        Ok(Self {
            tool_definitions,
            bound_argument_types,
        })
    }

    fn validate_workflow_tool_bindings(
        &self,
        workflow: &Workflow,
        type_inference_context: &TypeInferenceContext,
    ) -> Result<(), WorkflowRuntimeError> {
        for declaration in workflow.declarations() {
            let crate::dsl::Declaration::Agent(agent_declaration) = declaration else {
                continue;
            };

            let Some(tools_expression) = agent_declaration.expression_property(AgentExpressionPropertyName::Tools) else {
                continue;
            };

            let parsed_tool_bindings = tools_expression.parse_agent_tools_expression(agent_declaration, self)?;

            for tool_binding in parsed_tool_bindings {
                tool_binding.validate_bound_arguments(agent_declaration, self, type_inference_context)?;
            }
        }

        Ok(())
    }

    fn tool_definition(&self, tool_name: &str) -> Option<&ToolDefinition> {
        self.tool_definitions.get(tool_name)
    }

    fn bound_argument_types(&self, tool_name: &str) -> Option<&HashMap<String, WorkflowType>> {
        self.bound_argument_types.get(tool_name)
    }
}

impl AgentToolBinding {
    fn validate_bound_arguments(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_tool_catalog: &RuntimeToolCatalog,
        type_inference_context: &TypeInferenceContext,
    ) -> Result<(), WorkflowRuntimeError> {
        let Some(tool_definition) = runtime_tool_catalog.tool_definition(&self.tool_name) else {
            return Ok(());
        };

        self.validate_known_bound_arguments(agent_declaration, runtime_tool_catalog)?;
        self.validate_bound_argument_types(agent_declaration, runtime_tool_catalog, type_inference_context)?;

        let required_bound_argument_names = tool_definition.required_bound_argument_names()?;

        if required_bound_argument_names.is_empty() {
            return Ok(());
        }

        let provided_bound_argument_names = self
            .argument_expressions
            .iter()
            .map(|argument_expression| argument_expression.argument_name.clone())
            .collect::<HashSet<_>>();

        let missing_bound_argument_names = required_bound_argument_names
            .into_iter()
            .filter(|required_argument_name| !provided_bound_argument_names.contains(required_argument_name))
            .collect::<Vec<_>>();

        if missing_bound_argument_names.is_empty() {
            return Ok(());
        }

        let formatted_missing_argument_names = missing_bound_argument_names
            .iter()
            .map(|argument_name| format!("`{argument_name}`"))
            .collect::<Vec<_>>()
            .join(", ");

        Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: AgentExpressionPropertyName::Tools.as_str().to_string(),
            message: format!(
                "tool `tool.{}` is missing required bound argument(s): {}",
                self.tool_name, formatted_missing_argument_names
            ),
        })
    }

    fn validate_known_bound_arguments(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_tool_catalog: &RuntimeToolCatalog,
    ) -> Result<(), WorkflowRuntimeError> {
        let Some(bound_argument_types) = runtime_tool_catalog.bound_argument_types(&self.tool_name) else {
            return Ok(());
        };

        for argument_expression in &self.argument_expressions {
            if bound_argument_types.contains_key(&argument_expression.argument_name) {
                continue;
            }

            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: format!(
                    "tool `tool.{}` does not define bound argument `{}`",
                    self.tool_name, argument_expression.argument_name
                ),
            });
        }

        Ok(())
    }

    fn validate_bound_argument_types(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_tool_catalog: &RuntimeToolCatalog,
        type_inference_context: &TypeInferenceContext,
    ) -> Result<(), WorkflowRuntimeError> {
        let Some(bound_argument_types) = runtime_tool_catalog.bound_argument_types(&self.tool_name) else {
            return Ok(());
        };

        for argument_expression in &self.argument_expressions {
            let Some(expected_argument_type) = bound_argument_types.get(&argument_expression.argument_name) else {
                continue;
            };

            let actual_argument_type = argument_expression.expression.infer_type(
                type_inference_context,
                &format!(
                    "tool `tool.{}` bound argument `{}`",
                    self.tool_name, argument_expression.argument_name
                ),
            )?;

            if ensure_type_matches(expected_argument_type, &actual_argument_type) {
                continue;
            }

            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: format!(
                    "tool `tool.{}` bound argument `{}` expects {}, found {}",
                    self.tool_name, argument_expression.argument_name, expected_argument_type, actual_argument_type
                ),
            });
        }

        Ok(())
    }
}

trait ToolDefinitionExt {
    fn required_bound_argument_names(&self) -> Result<Vec<String>, WorkflowRuntimeError>;
}

impl ToolDefinitionExt for ToolDefinition {
    fn required_bound_argument_names(&self) -> Result<Vec<String>, WorkflowRuntimeError> {
        let Some(bound_parameters_schema) = &self.bound_parameters_schema else {
            return Ok(Vec::new());
        };

        let schema_value = serde_json::to_value(bound_parameters_schema).map_err(|error| WorkflowRuntimeError::Other {
            message: format!("failed to inspect bound parameter schema for tool `tool.{}`: {error}", self.name),
        })?;

        let required_values = schema_value
            .as_object()
            .and_then(|schema_object| schema_object.get("required"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut required_bound_argument_names = required_values
            .into_iter()
            .filter_map(|required_value| required_value.as_str().map(str::to_string))
            .collect::<Vec<_>>();

        required_bound_argument_names.sort();
        required_bound_argument_names.dedup();

        Ok(required_bound_argument_names)
    }
}

impl AgentForLoop {
    fn local_bindings_for_iteration_item(
        &self,
        iterable_item: &Value,
        agent_name: &str,
    ) -> Result<HashMap<String, Value>, WorkflowRuntimeError> {
        self.pattern.local_bindings_for_iteration_item(iterable_item, agent_name)
    }
}

impl AgentForLoopPattern {
    fn local_bindings_for_iteration_item(
        &self,
        iterable_item: &Value,
        agent_name: &str,
    ) -> Result<HashMap<String, Value>, WorkflowRuntimeError> {
        match self {
            Self::Identifier(iterator_name) => {
                let mut local_bindings = HashMap::new();
                local_bindings.insert(iterator_name.clone(), iterable_item.clone());

                Ok(local_bindings)
            }
            Self::ObjectDestructuring(field_names) => self.object_destructuring_bindings(iterable_item, field_names, agent_name),
        }
    }

    fn object_destructuring_bindings(
        &self,
        iterable_item: &Value,
        field_names: &[String],
        agent_name: &str,
    ) -> Result<HashMap<String, Value>, WorkflowRuntimeError> {
        let Some(iterable_object) = iterable_item.as_object() else {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: format!("for-loop iterable for agent `{agent_name}`"),
                message: format!(
                    "object destructuring in for-loop requires object items, found {}",
                    value_kind_name(iterable_item)
                ),
            });
        };

        let mut local_bindings = HashMap::new();

        for field_name in field_names {
            let Some(field_value) = iterable_object.get(field_name) else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: format!("for-loop iterable for agent `{agent_name}`"),
                    message: format!("object destructuring field `{field_name}` is missing from iterable item"),
                });
            };

            local_bindings.insert(field_name.clone(), field_value.clone());
        }

        Ok(local_bindings)
    }
}

pub struct WorkflowRuntime<Input, Output>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    workflow: Workflow,
    compiled_workflow: CompiledWorkflow,
    runtime_tools: Vec<DynamicTool>,
    phantom: PhantomData<(Input, Output)>,
}

impl<Input, Output> WorkflowRuntime<Input, Output>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    pub fn new(workflow: Workflow) -> Result<Self, WorkflowRuntimeError> {
        Self::new_with_runtime_tools(workflow, Vec::new())
    }

    pub(crate) fn new_with_runtime_tools(workflow: Workflow, runtime_tools: Vec<DynamicTool>) -> Result<Self, WorkflowRuntimeError> {
        let compiled_workflow = compile_workflow::<Input, Output>(&workflow)?;
        let runtime_tool_catalog = RuntimeToolCatalog::from_workflow_and_runtime_tools(&workflow, runtime_tools.as_slice())?;
        runtime_tool_catalog.validate_workflow_tool_bindings(&workflow, &compiled_workflow.execution_plan.type_inference_context())?;

        Ok(Self {
            workflow,
            compiled_workflow,
            runtime_tools,
            phantom: PhantomData,
        })
    }

    fn runtime_tool_catalog(&self) -> Result<RuntimeToolCatalog, WorkflowRuntimeError> {
        RuntimeToolCatalog::from_workflow_and_runtime_tools(&self.workflow, self.runtime_tools.as_slice())
    }

    pub fn from_file(workflow_path: impl AsRef<Path>) -> Result<Self, WorkflowRuntimeError> {
        let workflow_path = workflow_path.as_ref();
        let workflow_source = fs::read_to_string(workflow_path).map_err(|error| WorkflowRuntimeError::Other {
            message: format!("failed to read workflow file `{}`: {error}", workflow_path.display()),
        })?;

        let parsed_workflow = parse_workflow(&workflow_source).map_err(|parse_error| {
            let source_name = workflow_path.display().to_string();
            let rendered_parse_error = parse_error.render_with_source(&workflow_source, &source_name);

            WorkflowRuntimeError::ParseFailed {
                source: parse_error,
                details: rendered_parse_error,
            }
        })?;

        Self::new(parsed_workflow)
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
        self.execute_workflow_let_bindings(&mut runtime_state).await?;

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

    async fn execute_workflow_let_bindings(&self, runtime_state: &mut RuntimeState) -> Result<(), WorkflowRuntimeError> {
        for declaration in self.workflow.declarations() {
            let Declaration::Let(let_binding) = declaration else {
                continue;
            };

            let binding_value = self
                .evaluate_binding_expression(
                    &let_binding.value,
                    runtime_state,
                    HashMap::new(),
                    &format!("let binding `{}`", let_binding.name),
                )
                .await?;

            runtime_state.local_bindings.insert(let_binding.name.clone(), binding_value);
        }

        Ok(())
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
            tools_expression.parse_agent_tools_expression(agent_declaration, &self.runtime_tool_catalog()?)?
        } else {
            Vec::new()
        };

        let output_type = planned_agent.iteration_output_type.clone();
        let output_schema = workflow_type_to_schemars_schema(&output_type, agent_declaration.output_description())?;
        let config = build_agent_config(agent_declaration, runtime_state)?;

        Ok(PreparedAgentExecution {
            agent_name,
            agent_declaration,
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
            let mut local_bindings =
                agent_for_loop.local_bindings_for_iteration_item(iterable_item, &prepared_agent_execution.agent_name)?;
            local_bindings = self
                .evaluate_agent_let_bindings(prepared_agent_execution, runtime_state, local_bindings)
                .await?;

            let model_name = self.evaluate_agent_model_name(prepared_agent_execution, runtime_state, local_bindings.clone())?;

            let prompt = self.evaluate_agent_prompt(prepared_agent_execution, runtime_state, local_bindings.clone())?;
            let context = self.evaluate_agent_context(prepared_agent_execution, runtime_state, local_bindings.clone())?;
            let tools = self.evaluate_agent_tools(prepared_agent_execution, runtime_state, local_bindings)?;

            let pending_iteration_execution = async move {
                let agent_result = self
                    .run_agent_request(prepared_agent_execution, model_name, prompt, context, tools, runner)
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
        let local_bindings = self
            .evaluate_agent_let_bindings(prepared_agent_execution, runtime_state, HashMap::new())
            .await?;
        let model_name = self.evaluate_agent_model_name(prepared_agent_execution, runtime_state, local_bindings.clone())?;
        let prompt = self.evaluate_agent_prompt(prepared_agent_execution, runtime_state, local_bindings.clone())?;
        let context = self.evaluate_agent_context(prepared_agent_execution, runtime_state, local_bindings.clone())?;
        let tools = self.evaluate_agent_tools(prepared_agent_execution, runtime_state, local_bindings)?;
        let agent_result = self
            .run_agent_request(prepared_agent_execution, model_name, prompt, context, tools, runner)
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

    async fn evaluate_agent_let_bindings(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        mut local_bindings: HashMap<String, Value>,
    ) -> Result<HashMap<String, Value>, WorkflowRuntimeError> {
        for agent_property in &prepared_agent_execution.agent_declaration.properties {
            let AgentProperty::Let(let_binding) = agent_property else {
                continue;
            };

            let binding_value = self
                .evaluate_binding_expression(
                    &let_binding.value,
                    runtime_state,
                    local_bindings.clone(),
                    &format!(
                        "let binding `{}` for agent `{}`",
                        let_binding.name, prepared_agent_execution.agent_name
                    ),
                )
                .await?;

            local_bindings.insert(let_binding.name.clone(), binding_value);
        }

        Ok(local_bindings)
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
                definition_override: tool_binding.definition.clone(),
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

        Ok(normalize_prompt(prompt_value))
    }

    fn evaluate_agent_context(
        &self,
        prepared_agent_execution: &PreparedAgentExecution<'_>,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
    ) -> Result<Option<Value>, WorkflowRuntimeError> {
        let Some(context_expression) = prepared_agent_execution.context_expression else {
            return Ok(None);
        };

        let context_value = evaluate_expression(
            context_expression,
            &runtime_state_to_evaluation_context(runtime_state, local_bindings),
            &format!("context for agent `{}`", prepared_agent_execution.agent_name),
        )?;

        Ok(Some(context_value))
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
        context: Option<Value>,
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
            context,
            config: prepared_agent_execution.config.clone(),
            output_schema: prepared_agent_execution.output_schema.clone(),
            requested_tools: tools,
            runtime_tools: self.runtime_tools.clone(),
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

    async fn evaluate_binding_expression(
        &self,
        expression: &Expression,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
        context: &str,
    ) -> Result<Value, WorkflowRuntimeError> {
        match expression {
            Expression::ToolCall(tool_call) => self.execute_tool_call(tool_call, runtime_state, local_bindings, context).await,
            _ => evaluate_expression(
                expression,
                &runtime_state_to_evaluation_context(runtime_state, local_bindings),
                context,
            ),
        }
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ToolCall,
        runtime_state: &RuntimeState,
        local_bindings: HashMap<String, Value>,
        context: &str,
    ) -> Result<Value, WorkflowRuntimeError> {
        if tool_call.callee.root_keyword() != Some(ReferenceKeyword::Tool) || tool_call.callee.accesses.len() != 1 {
            return Err(WorkflowRuntimeError::ExpressionEvaluation {
                context: context.to_string(),
                message: "tool call callee must be a direct `tool.name` reference".to_string(),
            });
        }

        let tool_name = tool_call
            .callee
            .first_access_field()
            .expect("tool call callee should have one access")
            .to_string();
        let evaluation_context = runtime_state_to_evaluation_context(runtime_state, local_bindings);
        let input_value = Self::evaluate_tool_call_fields(&tool_call.input_fields, &evaluation_context, context)?;
        let binding_values = Self::evaluate_tool_call_fields(&tool_call.binding_fields, &evaluation_context, context)?;
        let runtime_tool = self.resolve_runtime_tool(tool_name.as_str())?;

        runtime_tool
            .execute_with_bound_arguments(Value::Object(input_value), binding_values)
            .await
            .map_err(|tool_error| WorkflowRuntimeError::Other {
                message: format!("tool `tool.{tool_name}` execution failed: {tool_error}"),
            })
    }

    fn evaluate_tool_call_fields(
        fields: &[crate::dsl::ObjectField],
        evaluation_context: &EvaluationContext,
        context: &str,
    ) -> Result<Map<String, Value>, WorkflowRuntimeError> {
        let mut field_values = Map::new();

        for field in fields {
            let field_value = evaluate_expression(&field.value, evaluation_context, context)?;
            field_values.insert(field.name.clone(), field_value);
        }

        Ok(field_values)
    }

    fn resolve_runtime_tool(&self, tool_name: &str) -> Result<Arc<dyn RuntimeTool>, WorkflowRuntimeError> {
        for registered_tool in registered_runtime_tools() {
            let tool_definition = registered_tool.definition().map_err(|tool_error| WorkflowRuntimeError::Other {
                message: format!("failed to inspect registered tool: {tool_error}"),
            })?;

            if tool_definition.name == tool_name {
                return Ok(registered_tool);
            }
        }

        for dynamic_tool in &self.runtime_tools {
            let dynamic_runtime_tool: Arc<dyn RuntimeTool> = Arc::new(dynamic_tool.clone());
            let tool_definition = dynamic_runtime_tool
                .definition()
                .map_err(|tool_error| WorkflowRuntimeError::Other {
                    message: format!("failed to inspect dynamic tool: {tool_error}"),
                })?;

            if tool_definition.name == tool_name {
                return Ok(dynamic_runtime_tool);
            }
        }

        Err(WorkflowRuntimeError::Other {
            message: format!("tool `tool.{tool_name}` is not available at runtime"),
        })
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

    fn parse_agent_tools_expression(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_tool_catalog: &RuntimeToolCatalog,
    ) -> Result<Vec<AgentToolBinding>, WorkflowRuntimeError> {
        let Expression::ArrayLiteral(tool_expressions) = self else {
            return Err(WorkflowRuntimeError::InvalidAgentProperty {
                agent_name: agent_declaration.name.clone(),
                property: AgentExpressionPropertyName::Tools.as_str().to_string(),
                message: "tools must be an array literal like [tool.weather, tool.weather(country: input.country)]".to_string(),
            });
        };

        let mut parsed_tools = Vec::new();

        for tool_expression in tool_expressions {
            parsed_tools.push(tool_expression.parse_agent_tool_binding(agent_declaration, runtime_tool_catalog)?);
        }

        Ok(parsed_tools)
    }

    fn parse_agent_tool_binding(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_tool_catalog: &RuntimeToolCatalog,
    ) -> Result<AgentToolBinding, WorkflowRuntimeError> {
        match self {
            Self::Reference(reference) => {
                let tool_name = reference.parse_agent_tool_name(agent_declaration)?;
                let definition = runtime_tool_catalog.tool_definition(&tool_name).cloned();

                Ok(AgentToolBinding {
                    tool_name,
                    argument_expressions: Vec::new(),
                    definition,
                })
            }
            Self::FunctionCall(function_call) => function_call.parse_agent_tool_call(agent_declaration, runtime_tool_catalog),
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
    fn parse_agent_tool_call(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_tool_catalog: &RuntimeToolCatalog,
    ) -> Result<AgentToolBinding, WorkflowRuntimeError> {
        let tool_name = self.callee.parse_agent_tool_name(agent_declaration)?;
        let definition = runtime_tool_catalog.tool_definition(&tool_name).cloned();
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
            definition,
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

pub async fn execute_workflow_file<Input, Output>(workflow_path: impl AsRef<Path>, input: Input) -> Result<Output, WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
    Output: DeserializeOwned + JsonSchema,
{
    WorkflowRuntime::<Input, Output>::from_file(workflow_path)?.run(input).await
}

pub async fn execute_workflow_without_input<Output>(workflow: &Workflow) -> Result<Output, WorkflowRuntimeError>
where
    Output: DeserializeOwned + JsonSchema,
{
    execute_workflow(workflow, ()).await
}

pub async fn execute_workflow_file_without_input<Output>(workflow_path: impl AsRef<Path>) -> Result<Output, WorkflowRuntimeError>
where
    Output: DeserializeOwned + JsonSchema,
{
    execute_workflow_file(workflow_path, ()).await
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

fn runtime_state_to_evaluation_context(runtime_state: &RuntimeState, local_bindings: HashMap<String, Value>) -> EvaluationContext {
    let mut merged_local_bindings = runtime_state.local_bindings.clone();

    for (binding_name, binding_value) in local_bindings {
        merged_local_bindings.insert(binding_name, binding_value);
    }

    EvaluationContext {
        input_values: runtime_state.input_values.clone(),
        secret_values: runtime_state.secret_values.clone(),
        agent_outputs: runtime_state.agent_outputs.clone(),
        agent_contexts: runtime_state.agent_contexts.clone(),
        local_bindings: merged_local_bindings,
    }
}
