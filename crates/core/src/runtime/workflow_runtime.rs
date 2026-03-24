use crate::dsl::{validate_workflow, InputDeclaration, SchemaDeclaration, SecretsDeclaration};
use crate::dsl::{
    AgentDeclaration, AgentProperty, CallArgument, Declaration, Expression, OutputDeclaration, TypeExpression, ValidationReport, Workflow,
};
use crate::runtime::error::WorkflowRuntimeError;
use crate::runtime::expression::{collect_agent_dependencies, evaluate_expression, EvaluationContext};
use crate::runtime::provider::{build_provider_index, ProviderConfig};
use crate::runtime::runner::{AgentExecutionRequest, AgentRunner, LoopAgentRunner};
use crate::runtime::type_inference::{infer_expression_type, TypeInferenceContext};
use crate::runtime::types::{
    ensure_type_matches, normalize_value_for_type, validate_value_against_type, value_kind_name, workflow_type_from_dsl,
    workflow_type_from_rust_schema, WorkflowType,
};
use engine_ai_agent::AgentConfig;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
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
    provider_index: HashMap<String, ProviderConfig>,
    input_type: Option<WorkflowType>,
    output_declaration: OutputDeclaration,
    workflow_output_type: WorkflowType,
    agent_declarations: Vec<AgentDeclaration>,
    agent_declaration_index: HashMap<String, AgentDeclaration>,
    agent_iteration_output_types: HashMap<String, WorkflowType>,
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
        let execution_order = self.resolve_agent_execution_order()?;

        for agent_name in execution_order {
            let agent_declaration = self
                .compiled_workflow
                .agent_declaration_index
                .get(&agent_name)
                .expect("agent should exist in declaration index")
                .clone();

            self.execute_agent(&agent_declaration, &mut runtime_state, runner).await?;
        }

        let workflow_output_value = self.evaluate_workflow_output(&runtime_state)?;

        validate_value_against_type(&workflow_output_value, &self.compiled_workflow.workflow_output_type).map_err(|message| {
            WorkflowRuntimeError::OutputTypeMismatch {
                expected: self.compiled_workflow.workflow_output_type.to_string(),
                found: format!("invalid runtime output: {message}"),
            }
        })?;

        serde_json::from_value::<Output>(workflow_output_value)
            .map_err(|source| WorkflowRuntimeError::OutputDeserializationFailed { source })
    }

    fn resolve_input_values(&self, serialized_input: &Value) -> Result<Map<String, Value>, WorkflowRuntimeError> {
        if let Some(input_type) = &self.compiled_workflow.input_type {
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

    fn resolve_agent_execution_order(&self) -> Result<Vec<String>, WorkflowRuntimeError> {
        let declaration_order = self
            .compiled_workflow
            .agent_declarations
            .iter()
            .map(|agent_declaration| agent_declaration.name.clone())
            .collect::<Vec<_>>();

        let mut dependency_index = HashMap::<String, HashSet<String>>::new();

        for agent_declaration in &self.compiled_workflow.agent_declarations {
            dependency_index.insert(agent_declaration.name.clone(), collect_dependencies_for_agent(agent_declaration));
        }

        let mut resolved_agents = HashSet::<String>::new();
        let mut ordered_agents = Vec::<String>::new();
        let mut unresolved_agents = declaration_order.iter().cloned().collect::<HashSet<_>>();

        while !unresolved_agents.is_empty() {
            let mut iteration_progress = false;

            for agent_name in &declaration_order {
                if !unresolved_agents.contains(agent_name) {
                    continue;
                }

                let dependencies = dependency_index
                    .get(agent_name)
                    .expect("dependency index should include all agents");

                if dependencies.iter().any(|dependency| !resolved_agents.contains(dependency)) {
                    continue;
                }

                unresolved_agents.remove(agent_name);
                resolved_agents.insert(agent_name.clone());
                ordered_agents.push(agent_name.clone());
                iteration_progress = true;
            }

            if iteration_progress {
                continue;
            }

            let mut blocked_agents = unresolved_agents.into_iter().collect::<Vec<_>>();
            blocked_agents.sort();

            return Err(WorkflowRuntimeError::Other {
                message: format!(
                    "failed to resolve agent execution order; blocked agents: {}",
                    blocked_agents.join(", ")
                ),
            });
        }

        Ok(ordered_agents)
    }

    async fn execute_agent<RunnerType>(
        &self,
        agent_declaration: &AgentDeclaration,
        runtime_state: &mut RuntimeState,
        runner: &RunnerType,
    ) -> Result<(), WorkflowRuntimeError>
    where
        RunnerType: AgentRunner,
    {
        let (provider_name, model_name) = parse_agent_model_binding(agent_declaration)?;

        let Some(provider_config) = self.compiled_workflow.provider_index.get(&provider_name) else {
            return Err(WorkflowRuntimeError::ProviderConfiguration {
                provider_name,
                message: "provider referenced by model binding is not declared".to_string(),
            });
        };

        let agent_prompt_expression = required_agent_property_expression(agent_declaration, "prompt")?;
        let context_property_expression = optional_agent_property_expression(agent_declaration, "context");

        if optional_agent_property_expression(agent_declaration, "tools").is_some() {
            return Err(WorkflowRuntimeError::UnsupportedFeature {
                feature: format!("agent `{}` uses `tools`, which is not supported yet", agent_declaration.name),
            });
        }

        let base_config = build_agent_config(agent_declaration, runtime_state)?;
        let iteration_output_type = self
            .compiled_workflow
            .agent_iteration_output_types
            .get(&agent_declaration.name)
            .expect("agent iteration output type should exist")
            .clone();

        if let Some(for_loop) = &agent_declaration.for_loop {
            let iterable_value = evaluate_expression(
                &for_loop.iterable,
                &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
                &format!("for-loop iterable for agent `{}`", agent_declaration.name),
            )?;

            let Some(iterable_items) = iterable_value.as_array() else {
                return Err(WorkflowRuntimeError::ExpressionEvaluation {
                    context: format!("for-loop iterable for agent `{}`", agent_declaration.name),
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
                    &format!("prompt for agent `{}`", agent_declaration.name),
                )?;

                let prompt = normalize_prompt(prompt_value);
                let prompt = apply_optional_context_prefix(
                    prompt,
                    context_property_expression,
                    runtime_state,
                    local_bindings,
                    &agent_declaration.name,
                )?;

                let request = AgentExecutionRequest {
                    agent_name: agent_declaration.name.clone(),
                    provider_config: provider_config.clone(),
                    model_name: model_name.clone(),
                    prompt,
                    config: base_config.clone(),
                };

                let agent_result = runner.run_agent(&request).await?;
                let normalized_output = normalize_value_for_type(&agent_result.output, &iteration_output_type).map_err(|message| {
                    WorkflowRuntimeError::AgentOutputTypeMismatch {
                        agent_name: agent_declaration.name.clone(),
                        message,
                    }
                })?;

                validate_value_against_type(&normalized_output, &iteration_output_type).map_err(|message| {
                    WorkflowRuntimeError::AgentOutputTypeMismatch {
                        agent_name: agent_declaration.name.clone(),
                        message,
                    }
                })?;

                iteration_outputs.push(normalized_output);
                iteration_contexts.push(agent_result.context);
            }

            runtime_state
                .agent_outputs
                .insert(agent_declaration.name.clone(), Value::Array(iteration_outputs));

            runtime_state
                .agent_contexts
                .insert(agent_declaration.name.clone(), Value::Array(iteration_contexts));

            return Ok(());
        }

        let prompt_value = evaluate_expression(
            agent_prompt_expression,
            &runtime_state_to_evaluation_context(runtime_state, HashMap::new()),
            &format!("prompt for agent `{}`", agent_declaration.name),
        )?;

        let prompt = normalize_prompt(prompt_value);
        let prompt = apply_optional_context_prefix(
            prompt,
            context_property_expression,
            runtime_state,
            HashMap::new(),
            &agent_declaration.name,
        )?;

        let request = AgentExecutionRequest {
            agent_name: agent_declaration.name.clone(),
            provider_config: provider_config.clone(),
            model_name,
            prompt,
            config: base_config,
        };

        let agent_result = runner.run_agent(&request).await?;
        let normalized_output = normalize_value_for_type(&agent_result.output, &iteration_output_type).map_err(|message| {
            WorkflowRuntimeError::AgentOutputTypeMismatch {
                agent_name: agent_declaration.name.clone(),
                message,
            }
        })?;

        validate_value_against_type(&normalized_output, &iteration_output_type).map_err(|message| {
            WorkflowRuntimeError::AgentOutputTypeMismatch {
                agent_name: agent_declaration.name.clone(),
                message,
            }
        })?;

        runtime_state
            .agent_outputs
            .insert(agent_declaration.name.clone(), normalized_output);

        runtime_state
            .agent_contexts
            .insert(agent_declaration.name.clone(), agent_result.context);

        Ok(())
    }

    fn evaluate_workflow_output(&self, runtime_state: &RuntimeState) -> Result<Value, WorkflowRuntimeError> {
        let mut output_fields = Map::new();
        let evaluation_context = runtime_state_to_evaluation_context(runtime_state, HashMap::new());

        for output_field in &self.compiled_workflow.output_declaration.fields {
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
    let validation_report = validate_workflow(workflow);

    if validation_report.has_issues() {
        return Err(WorkflowRuntimeError::InvalidWorkflow {
            issues: render_validation_report(&validation_report),
        });
    }

    let provider_index = build_provider_index(workflow)?;
    let named_schema_types = collect_named_schema_types(workflow);
    let input_type = build_input_type(workflow.find_input(), &named_schema_types)?;
    let secrets_type = build_secrets_type(workflow.find_secrets(), &named_schema_types)?;
    let output_declaration = workflow
        .find_output()
        .ok_or_else(|| WorkflowRuntimeError::MissingDeclaration {
            message: "workflow requires an `output` block".to_string(),
        })?
        .clone();

    let (agent_declarations, agent_declaration_index) = collect_agent_declarations(workflow);
    let (agent_iteration_output_types, agent_final_output_types) = collect_agent_output_types(&agent_declarations, &named_schema_types)?;

    let workflow_output_type = infer_workflow_output_type(
        &output_declaration,
        input_type.clone(),
        secrets_type.clone(),
        &agent_final_output_types,
    )?;

    validate_input_type_compatibility::<Input>(input_type.as_ref())?;
    validate_output_type_compatibility::<Output>(&workflow_output_type)?;

    Ok(CompiledWorkflow {
        provider_index,
        input_type,
        output_declaration,
        workflow_output_type,
        agent_declarations,
        agent_declaration_index,
        agent_iteration_output_types,
    })
}

fn collect_named_schema_types(workflow: &Workflow) -> HashMap<String, TypeExpression> {
    let mut named_schema_types = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Schema(SchemaDeclaration { name, fields, span: _ }) = declaration else {
            continue;
        };

        named_schema_types.insert(name.clone(), TypeExpression::Object(fields.clone()));
    }

    named_schema_types
}

fn build_input_type(
    input_declaration: Option<&InputDeclaration>,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<Option<WorkflowType>, WorkflowRuntimeError> {
    let Some(input_declaration) = input_declaration else {
        return Ok(None);
    };

    let object_type_expression = TypeExpression::Object(input_declaration.fields.clone());
    let input_type = workflow_type_from_dsl(&object_type_expression, named_schema_types)?;

    Ok(Some(input_type))
}

fn build_secrets_type(
    secrets_declaration: Option<&SecretsDeclaration>,
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<Option<WorkflowType>, WorkflowRuntimeError> {
    let Some(secrets_declaration) = secrets_declaration else {
        return Ok(None);
    };

    let object_type_expression = TypeExpression::Object(secrets_declaration.fields.clone());
    let secrets_type = workflow_type_from_dsl(&object_type_expression, named_schema_types)?;

    Ok(Some(secrets_type))
}

fn collect_agent_declarations(workflow: &Workflow) -> (Vec<AgentDeclaration>, HashMap<String, AgentDeclaration>) {
    let mut declarations = Vec::new();
    let mut index = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        declarations.push(agent_declaration.clone());
        index.insert(agent_declaration.name.clone(), agent_declaration.clone());
    }

    (declarations, index)
}

fn collect_agent_output_types(
    agent_declarations: &[AgentDeclaration],
    named_schema_types: &HashMap<String, TypeExpression>,
) -> Result<(HashMap<String, WorkflowType>, HashMap<String, WorkflowType>), WorkflowRuntimeError> {
    let mut iteration_output_types = HashMap::new();
    let mut final_output_types = HashMap::new();

    for agent_declaration in agent_declarations {
        let iteration_output_type = if let Some(output_type_expression) = optional_agent_output_type(agent_declaration) {
            workflow_type_from_dsl(output_type_expression, named_schema_types)?
        } else {
            WorkflowType::String
        };

        let final_output_type = if agent_declaration.for_loop.is_some() {
            WorkflowType::Array {
                item_type: Box::new(iteration_output_type.clone()),
                fixed_length: None,
            }
            .normalize()
        } else {
            iteration_output_type.clone()
        };

        iteration_output_types.insert(agent_declaration.name.clone(), iteration_output_type);
        final_output_types.insert(agent_declaration.name.clone(), final_output_type);
    }

    Ok((iteration_output_types, final_output_types))
}

fn optional_agent_output_type(agent_declaration: &AgentDeclaration) -> Option<&TypeExpression> {
    for agent_property in &agent_declaration.properties {
        if let AgentProperty::Output(output_type_expression) = agent_property {
            return Some(output_type_expression);
        }
    }

    None
}

fn infer_workflow_output_type(
    output_declaration: &OutputDeclaration,
    input_type: Option<WorkflowType>,
    secrets_type: Option<WorkflowType>,
    agent_output_types: &HashMap<String, WorkflowType>,
) -> Result<WorkflowType, WorkflowRuntimeError> {
    let inference_context = TypeInferenceContext {
        input_type,
        secrets_type,
        agent_output_types: agent_output_types.clone(),
        local_binding_types: HashMap::new(),
    };

    let mut output_fields = BTreeMap::new();

    for output_field in &output_declaration.fields {
        let field_type = infer_expression_type(&output_field.value, &inference_context, "workflow output type inference")?;
        output_fields.insert(output_field.name.clone(), field_type);
    }

    Ok(WorkflowType::Object(output_fields).normalize())
}

fn validate_input_type_compatibility<Input>(input_type: Option<&WorkflowType>) -> Result<(), WorkflowRuntimeError>
where
    Input: Serialize + JsonSchema,
{
    let rust_input_type = workflow_type_from_rust_schema::<Input>()?;

    if let Some(expected_input_type) = input_type {
        if ensure_type_matches(expected_input_type, &rust_input_type) {
            return Ok(());
        }

        Err(WorkflowRuntimeError::InputTypeMismatch {
            expected: expected_input_type.to_string(),
            found: rust_input_type.to_string(),
        })
    } else {
        if is_no_input_type(&rust_input_type) {
            return Ok(());
        }

        Err(WorkflowRuntimeError::InputTypeMismatch {
            expected: "no input".to_string(),
            found: rust_input_type.to_string(),
        })
    }
}

fn validate_output_type_compatibility<Output>(workflow_output_type: &WorkflowType) -> Result<(), WorkflowRuntimeError>
where
    Output: DeserializeOwned + JsonSchema,
{
    let rust_output_type = workflow_type_from_rust_schema::<Output>()?;

    if ensure_type_matches(workflow_output_type, &rust_output_type) {
        return Ok(());
    }

    Err(WorkflowRuntimeError::OutputTypeMismatch {
        expected: workflow_output_type.to_string(),
        found: rust_output_type.to_string(),
    })
}

fn is_no_input_type(workflow_type: &WorkflowType) -> bool {
    match workflow_type {
        WorkflowType::Null => true,
        WorkflowType::Object(fields) => fields.is_empty(),
        WorkflowType::String
        | WorkflowType::Integer
        | WorkflowType::Float
        | WorkflowType::Boolean
        | WorkflowType::StringEnum(_)
        | WorkflowType::Array {
            item_type: _,
            fixed_length: _,
        }
        | WorkflowType::Tuple(_)
        | WorkflowType::Union(_) => false,
    }
}

fn render_validation_report(validation_report: &ValidationReport) -> String {
    validation_report
        .issues_with_spans()
        .map(|(validation_issue, span)| match span {
            Some(span) => format!("- {validation_issue:?} at {}:{}", span.start.line, span.start.column),
            None => format!("- {validation_issue:?}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_dependencies_for_agent(agent_declaration: &AgentDeclaration) -> HashSet<String> {
    let mut dependencies = HashSet::new();

    if let Some(for_loop) = &agent_declaration.for_loop {
        collect_agent_dependencies(&for_loop.iterable, &mut dependencies);
    }

    for agent_property in &agent_declaration.properties {
        match agent_property {
            AgentProperty::Model(expression)
            | AgentProperty::Prompt(expression)
            | AgentProperty::Context(expression)
            | AgentProperty::Inference(expression)
            | AgentProperty::Tools(expression)
            | AgentProperty::Custom {
                name: _,
                value: expression,
            } => {
                collect_agent_dependencies(expression, &mut dependencies);
            }
            AgentProperty::Output(_) => {}
        }
    }

    dependencies.remove(&agent_declaration.name);

    dependencies
}

fn parse_agent_model_binding(agent_declaration: &AgentDeclaration) -> Result<(String, String), WorkflowRuntimeError> {
    let model_expression = required_agent_property_expression(agent_declaration, "model")?;
    let Expression::FunctionCall(model_call) = model_expression else {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: "model".to_string(),
            message: "model must be a provider call like provider_name(\"model\")".to_string(),
        });
    };

    if !model_call.callee.accesses.is_empty() {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: "model".to_string(),
            message: "model function callee must be a direct provider name".to_string(),
        });
    }

    let provider_name = model_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: "model".to_string(),
            message: "model provider name must be an identifier".to_string(),
        })?
        .to_string();

    let mut detected_model_names = Vec::<String>::new();

    for call_argument in &model_call.arguments {
        match call_argument {
            CallArgument::Positional(expression) => {
                if let Expression::StringLiteral(model_name) = expression {
                    detected_model_names.push(model_name.clone());
                }
            }
            CallArgument::Named(named_argument) if named_argument.name == "model" => {
                let Expression::StringLiteral(model_name) = &named_argument.value else {
                    return Err(WorkflowRuntimeError::InvalidAgentProperty {
                        agent_name: agent_declaration.name.clone(),
                        property: "model".to_string(),
                        message: "named `model` argument must be a string".to_string(),
                    });
                };

                detected_model_names.push(model_name.clone());
            }
            CallArgument::Named(_) => {}
        }
    }

    if detected_model_names.is_empty() {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: "model".to_string(),
            message: "missing model name argument".to_string(),
        });
    }

    let model_name = detected_model_names[0].clone();

    if detected_model_names.iter().any(|candidate| candidate != &model_name) {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_declaration.name.clone(),
            property: "model".to_string(),
            message: "ambiguous model name arguments".to_string(),
        });
    }

    Ok((provider_name, model_name))
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

    if let Some(max_tokens) = parse_optional_u64(inference_fields, "max_tokens")? {
        config = config.with_max_tokens(u64_to_usize(max_tokens, "max_tokens", &agent_declaration.name)?);
    }

    if let Some(temperature) = parse_optional_f32(inference_fields, "temperature")? {
        config = config.with_temperature(temperature);
    }

    if let Some(top_p) = parse_optional_f32(inference_fields, "top_p")? {
        config = config.with_top_p(top_p);
    }

    if let Some(top_k) = parse_optional_u64(inference_fields, "top_k")? {
        config = config.with_top_k(u64_to_u32(top_k, "top_k", &agent_declaration.name)?);
    }

    if let Some(frequency_penalty) = parse_optional_f32(inference_fields, "frequency_penalty")? {
        config = config.with_frequency_penalty(frequency_penalty);
    }

    if let Some(presence_penalty) = parse_optional_f32(inference_fields, "presence_penalty")? {
        config = config.with_presence_penalty(presence_penalty);
    }

    if let Some(repeat_penalty) = parse_optional_f32(inference_fields, "repeat_penalty")? {
        config = config.with_repeat_penalty(repeat_penalty);
    }

    if let Some(seed) = parse_optional_i32(inference_fields, "seed", &agent_declaration.name)? {
        config = config.with_seed(seed);
    }

    if let Some(stuck_threshold) = parse_optional_u64(inference_fields, "stuck_threshold")? {
        config = config.with_stuck_threshold(u64_to_usize(stuck_threshold, "stuck_threshold", &agent_declaration.name)?);
    }

    if let Some(provider_max_retries) = parse_optional_u64(inference_fields, "provider_max_retries")? {
        config = config.with_provider_max_retries(u64_to_usize(provider_max_retries, "provider_max_retries", &agent_declaration.name)?);
    }

    if let Some(provider_retry_base_delay_ms) = parse_optional_u64(inference_fields, "provider_retry_base_delay_ms")? {
        config = config.with_provider_retry_base_delay_ms(provider_retry_base_delay_ms);
    }

    Ok(config)
}

fn parse_optional_u64(inference_fields: &Map<String, Value>, field_name: &str) -> Result<Option<u64>, WorkflowRuntimeError> {
    let Some(field_value) = inference_fields.get(field_name) else {
        return Ok(None);
    };

    let Some(parsed_value) = field_value.as_u64() else {
        return Err(WorkflowRuntimeError::Other {
            message: format!("inference `{field_name}` must be a non-negative integer"),
        });
    };

    Ok(Some(parsed_value))
}

fn parse_optional_i32(
    inference_fields: &Map<String, Value>,
    field_name: &str,
    agent_name: &str,
) -> Result<Option<i32>, WorkflowRuntimeError> {
    let Some(field_value) = inference_fields.get(field_name) else {
        return Ok(None);
    };

    let Some(parsed_value) = field_value.as_i64() else {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_name.to_string(),
            property: "inference".to_string(),
            message: format!("`{field_name}` must be an integer"),
        });
    };

    let parsed_value = i32::try_from(parsed_value).map_err(|_| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_name.to_string(),
        property: "inference".to_string(),
        message: format!("`{field_name}` exceeds i32 range"),
    })?;

    Ok(Some(parsed_value))
}

fn parse_optional_f32(inference_fields: &Map<String, Value>, field_name: &str) -> Result<Option<f32>, WorkflowRuntimeError> {
    let Some(field_value) = inference_fields.get(field_name) else {
        return Ok(None);
    };

    let parsed_value = serde_json::from_value::<f32>(field_value.clone()).map_err(|_| WorkflowRuntimeError::Other {
        message: format!("inference `{field_name}` must be numeric"),
    })?;

    if !parsed_value.is_finite() {
        return Err(WorkflowRuntimeError::Other {
            message: format!("inference `{field_name}` must be numeric"),
        });
    }

    Ok(Some(parsed_value))
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

fn u64_to_usize(value: u64, field_name: &str, agent_name: &str) -> Result<usize, WorkflowRuntimeError> {
    usize::try_from(value).map_err(|_| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_name.to_string(),
        property: "inference".to_string(),
        message: format!("`{field_name}` exceeds usize range"),
    })
}

fn u64_to_u32(value: u64, field_name: &str, agent_name: &str) -> Result<u32, WorkflowRuntimeError> {
    u32::try_from(value).map_err(|_| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_name.to_string(),
        property: "inference".to_string(),
        message: format!("`{field_name}` exceeds u32 range"),
    })
}
