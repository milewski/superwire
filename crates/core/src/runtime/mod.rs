use crate::dsl::{
    parse_workflow, validate_workflow, AgentDeclaration, AgentProperty, CallArgument, Declaration, DslParseError, Expression, FunctionCall,
    ProviderDeclaration, Reference, ReferenceKeyword, ReferenceRoot, SourceSpan, StringTemplatePart, TypeExpression, TypedField,
    ValidationIssue, Workflow,
};
use async_trait::async_trait;
use engine_ai_agent::{
    Agent, AgentConfig, Context, LoopExecutor, OllamaProvider, OpenAIProvider, Provider, ProviderError, ProviderResponse, StopReason,
    ToolCall, ToolDefinition,
};
use petgraph::algo::{kosaraju_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationProblem {
    pub issue: ValidationIssue,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Error)]
pub enum WorkflowRuntimeError {
    #[error("failed to parse workflow: {0}")]
    Parse(#[from] DslParseError),

    #[error("workflow validation failed")]
    ValidationFailed { problems: Vec<ValidationProblem> },

    #[error("agent '{agent_name}' is missing a model property")]
    MissingModelExpression { agent_name: String },

    #[error("agent '{agent_name}' has an invalid model property")]
    InvalidModelExpression { agent_name: String },

    #[error("agent '{agent_name}' references unknown provider '{provider_name}'")]
    MissingProviderDeclaration { agent_name: String, provider_name: String },

    #[error("agent '{agent_name}' uses for-loop execution which is not supported by this runtime yet")]
    UnsupportedForLoop { agent_name: String },

    #[error("tool keyword references are not supported in {context}")]
    UnsupportedToolKeywordReference { context: String },

    #[error("function '{function_name}' is not supported in {context}")]
    UnsupportedFunctionCall { function_name: String, context: String },

    #[error("unknown reference identifier '{identifier}' in {context}")]
    UnknownReferenceIdentifier { identifier: String, context: String },

    #[error("invalid reference '{reference_path}': cannot access field '{field_name}' in {context}")]
    InvalidReferencePath {
        reference_path: String,
        field_name: String,
        context: String,
    },

    #[error("provider factory failed: {message}")]
    ProviderFactoryFailed { message: String },

    #[error("failed to construct loop executor for agent '{agent_name}': {message}")]
    LoopExecutorCreationFailed { agent_name: String, message: String },

    #[error("agent '{agent_name}' execution failed: {message}")]
    AgentExecutionFailed { agent_name: String, message: String },

    #[error("agent '{agent_name}' output type mismatch: {message}")]
    AgentOutputTypeMismatch { agent_name: String, message: String },

    #[error("agent dependency cycle detected: {agent_names:?}")]
    DependencyCycle { agent_names: Vec<String> },

    #[error("invalid numeric literal '{literal}' in {context}")]
    InvalidNumberLiteral { literal: String, context: String },
}

#[derive(Debug, Clone)]
pub struct WorkflowExecutionResult {
    pub output: Value,
    pub agent_outputs_by_name: HashMap<String, Value>,
    pub agent_contexts_by_name: HashMap<String, Context>,
}

struct ExecutionScope<'scope> {
    input_values: &'scope Value,
    secret_values: &'scope Value,
    agent_outputs_by_name: &'scope HashMap<String, Value>,
}

struct ModelBinding {
    provider_name: String,
    model_name: String,
}

pub trait WorkflowProviderFactory: Send + Sync {
    fn build_provider(
        &self,
        agent_name: &str,
        provider_name: &str,
        provider_settings: &Map<String, Value>,
        model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError>;
}

#[derive(Clone)]
pub struct DynamicProvider {
    inner_provider: Arc<dyn Provider + Send + Sync>,
}

impl DynamicProvider {
    #[must_use]
    pub fn new<ProviderType>(provider: ProviderType) -> Self
    where
        ProviderType: Provider + Send + Sync + 'static,
    {
        Self {
            inner_provider: Arc::new(provider),
        }
    }
}

#[async_trait]
impl Provider for DynamicProvider {
    async fn generate(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<ProviderResponse, ProviderError> {
        self.inner_provider.generate(context, tools, config).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderDriver {
    OpenAi,
    Ollama,
}

impl ProviderDriver {
    fn from_driver_name(driver_name: &str) -> Option<Self> {
        match driver_name {
            "openai" => Some(Self::OpenAi),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultProviderFactory;

impl WorkflowProviderFactory for DefaultProviderFactory {
    fn build_provider(
        &self,
        _agent_name: &str,
        provider_name: &str,
        provider_settings: &Map<String, Value>,
        model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError> {
        let driver_name = string_setting(provider_settings, "driver").ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("provider '{provider_name}' is missing a string `driver` setting"),
        })?;

        let provider_driver =
            ProviderDriver::from_driver_name(driver_name.as_str()).ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
                message: format!("provider '{provider_name}' has unknown driver '{driver_name}'. Supported drivers: openai, ollama"),
            })?;

        match provider_driver {
            ProviderDriver::OpenAi => {
                let openai_api_key = string_setting(provider_settings, "api_key").or_else(|| std::env::var("OPENAI_API_KEY").ok());
                let openai_base_url =
                    string_setting(provider_settings, "base_url").or_else(|| string_setting(provider_settings, "api_endpoint"));

                let openai_provider = match (openai_base_url, openai_api_key) {
                    (Some(base_url), Some(api_key)) => OpenAIProvider::new_with_base_url(base_url, api_key, model_name),
                    (Some(base_url), None) => OpenAIProvider::new_local(base_url, model_name),
                    (None, Some(api_key)) => OpenAIProvider::new(api_key, model_name),
                    (None, None) => {
                        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
                            message: format!(
                                "provider '{provider_name}' with openai driver requires `api_key`, `base_url`, `api_endpoint`, or OPENAI_API_KEY"
                            ),
                        });
                    }
                };

                Ok(DynamicProvider::new(openai_provider))
            }
            ProviderDriver::Ollama => {
                let endpoint_setting = string_setting(provider_settings, "api_endpoint")
                    .or_else(|| string_setting(provider_settings, "base_url"))
                    .or_else(|| string_setting(provider_settings, "host"));

                let (host, port) = if let Some(endpoint_value) = endpoint_setting {
                    parse_ollama_endpoint(endpoint_value.as_str())?
                } else {
                    ("http://localhost".to_owned(), 11434)
                };

                let ollama_provider = OllamaProvider::new(host, port, model_name.to_owned());

                Ok(DynamicProvider::new(ollama_provider))
            }
        }
    }
}

fn string_setting(provider_settings: &Map<String, Value>, setting_name: &str) -> Option<String> {
    provider_settings.get(setting_name).and_then(Value::as_str).map(str::to_owned)
}

fn parse_ollama_endpoint(endpoint_value: &str) -> Result<(String, u16), WorkflowRuntimeError> {
    let trimmed_endpoint = endpoint_value.trim();

    if trimmed_endpoint.is_empty() {
        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
            message: "ollama endpoint cannot be empty".to_owned(),
        });
    }

    let (scheme, endpoint_without_scheme) = if let Some(stripped_endpoint) = trimmed_endpoint.strip_prefix("http://") {
        ("http", stripped_endpoint)
    } else if let Some(stripped_endpoint) = trimmed_endpoint.strip_prefix("https://") {
        ("https", stripped_endpoint)
    } else {
        ("http", trimmed_endpoint)
    };

    let host_and_port = endpoint_without_scheme
        .split('/')
        .next()
        .ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("invalid ollama endpoint '{endpoint_value}'"),
        })?;

    if host_and_port.is_empty() {
        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("invalid ollama endpoint '{endpoint_value}'"),
        });
    }

    let mut host_and_port_segments = host_and_port.splitn(2, ':');
    let host_segment = host_and_port_segments.next().unwrap_or_default();

    if host_segment.is_empty() {
        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("invalid ollama endpoint '{endpoint_value}'"),
        });
    }

    let port_segment = host_and_port_segments.next();

    let port = if let Some(port_segment) = port_segment {
        port_segment
            .parse::<u16>()
            .map_err(|_| WorkflowRuntimeError::ProviderFactoryFailed {
                message: format!("invalid ollama port in endpoint '{endpoint_value}'"),
            })?
    } else {
        11434
    };

    let host = format!("{scheme}://{host_segment}");

    Ok((host, port))
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedProviderFactory {
    outputs_by_agent_name: HashMap<String, Value>,
}

impl ScriptedProviderFactory {
    #[must_use]
    pub fn new(outputs_by_agent_name: HashMap<String, Value>) -> Self {
        Self { outputs_by_agent_name }
    }
}

impl WorkflowProviderFactory for ScriptedProviderFactory {
    fn build_provider(
        &self,
        agent_name: &str,
        _provider_name: &str,
        _provider_settings: &Map<String, Value>,
        _model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError> {
        let output_value =
            self.outputs_by_agent_name
                .get(agent_name)
                .cloned()
                .ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
                    message: format!("scripted output is missing for agent '{agent_name}'"),
                })?;

        Ok(DynamicProvider::new(ScriptedProvider::new(output_value)))
    }
}

#[derive(Debug, Clone)]
struct ScriptedProvider {
    output_value: Value,
}

impl ScriptedProvider {
    fn new(output_value: Value) -> Self {
        Self { output_value }
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn generate(
        &self,
        _context: &Context,
        _tools: &[ToolDefinition],
        _config: &AgentConfig,
    ) -> Result<ProviderResponse, ProviderError> {
        let finalize_tool_call = ToolCall {
            id: "scripted-finalize".to_owned(),
            name: "finalize".to_owned(),
            arguments: serde_json::json!({
                "output": {
                    "type": "success",
                    "answer": self.output_value.clone(),
                }
            }),
        };

        Ok(ProviderResponse {
            tool_calls: vec![finalize_tool_call],
            text: None,
            stop_reason: StopReason::ToolCalls,
            usage: None,
        })
    }
}

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

            let loop_executor =
                LoopExecutor::<DynamicProvider, Value>::new().map_err(|error| WorkflowRuntimeError::LoopExecutorCreationFailed {
                    agent_name: agent_name.clone(),
                    message: error.to_string(),
                })?;

            let run_result = Agent::new(loop_executor, provider)
                .with_config(self.agent_config.clone())
                .run(prompt_text)
                .await
                .map_err(|error| WorkflowRuntimeError::AgentExecutionFailed {
                    agent_name: agent_name.clone(),
                    message: error.to_string(),
                })?;

            if let Some(output_type_expression) = find_output_type_expression(agent_declaration.properties.as_slice()) {
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

fn evaluate_provider_settings(
    provider_declaration: &ProviderDeclaration,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Map<String, Value>, WorkflowRuntimeError> {
    let mut provider_settings = Map::new();

    for provider_property in &provider_declaration.properties {
        let property_context = format!("{evaluation_context} property '{}'", provider_property.name);
        let property_value = evaluate_expression(&provider_property.value, execution_scope, property_context.as_str())?;
        provider_settings.insert(provider_property.name.clone(), property_value);
    }

    Ok(provider_settings)
}

fn find_prompt_expression(agent_properties: &[AgentProperty]) -> Option<&Expression> {
    agent_properties.iter().find_map(|agent_property| {
        if let AgentProperty::Prompt(prompt_expression) = agent_property {
            Some(prompt_expression)
        } else {
            None
        }
    })
}

fn find_output_type_expression(agent_properties: &[AgentProperty]) -> Option<&TypeExpression> {
    agent_properties.iter().find_map(|agent_property| {
        if let AgentProperty::Output(output_type_expression) = agent_property {
            Some(output_type_expression)
        } else {
            None
        }
    })
}

fn extract_model_binding(agent_declaration: &AgentDeclaration) -> Result<ModelBinding, WorkflowRuntimeError> {
    let model_expression = agent_declaration
        .properties
        .iter()
        .find_map(|agent_property| {
            if let AgentProperty::Model(model_expression) = agent_property {
                Some(model_expression)
            } else {
                None
            }
        })
        .ok_or_else(|| WorkflowRuntimeError::MissingModelExpression {
            agent_name: agent_declaration.name.clone(),
        })?;

    let Expression::FunctionCall(model_call) = model_expression else {
        return Err(WorkflowRuntimeError::InvalidModelExpression {
            agent_name: agent_declaration.name.clone(),
        });
    };

    if !model_call.callee.accesses.is_empty() {
        return Err(WorkflowRuntimeError::InvalidModelExpression {
            agent_name: agent_declaration.name.clone(),
        });
    }

    let provider_name = model_call
        .callee
        .root
        .as_identifier()
        .ok_or_else(|| WorkflowRuntimeError::InvalidModelExpression {
            agent_name: agent_declaration.name.clone(),
        })?
        .to_owned();

    let model_name = extract_model_name(model_call).ok_or_else(|| WorkflowRuntimeError::InvalidModelExpression {
        agent_name: agent_declaration.name.clone(),
    })?;

    Ok(ModelBinding { provider_name, model_name })
}

fn extract_model_name(model_call: &FunctionCall) -> Option<String> {
    for call_argument in &model_call.arguments {
        match call_argument {
            CallArgument::Positional(Expression::StringLiteral(model_name)) => {
                return Some(model_name.clone());
            }
            CallArgument::Named(named_argument) if named_argument.name == "model" => {
                let Expression::StringLiteral(model_name) = &named_argument.value else {
                    return None;
                };

                return Some(model_name.clone());
            }
            CallArgument::Named(_) | CallArgument::Positional(_) => {}
        }
    }

    None
}

fn evaluate_workflow_output(
    workflow: &Workflow,
    input_values: &Value,
    secret_values: &Value,
    agent_outputs_by_name: &HashMap<String, Value>,
) -> Result<Value, WorkflowRuntimeError> {
    let Some(output_declaration) = workflow.find_output() else {
        return Ok(Value::Object(Map::new()));
    };

    let execution_scope = ExecutionScope {
        input_values,
        secret_values,
        agent_outputs_by_name,
    };
    let mut output_object = Map::new();

    for output_field in &output_declaration.fields {
        let output_field_context = format!("workflow output field '{}'", output_field.name);
        let output_field_value = evaluate_expression(&output_field.value, &execution_scope, output_field_context.as_str())?;
        output_object.insert(output_field.name.clone(), output_field_value);
    }

    Ok(Value::Object(output_object))
}

fn evaluate_expression(
    expression: &Expression,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    match expression {
        Expression::StringLiteral(string_value) => Ok(Value::String(string_value.clone())),
        Expression::StringTemplate(string_template) => {
            let mut rendered_string = String::new();

            for string_template_part in &string_template.parts {
                match string_template_part {
                    StringTemplatePart::Text(text_value) => {
                        rendered_string.push_str(text_value);
                    }
                    StringTemplatePart::Interpolation(interpolation_expression) => {
                        let interpolation_value = evaluate_expression(interpolation_expression, execution_scope, evaluation_context)?;

                        rendered_string.push_str(render_value_as_text(&interpolation_value).as_str());
                    }
                }
            }

            Ok(Value::String(rendered_string))
        }
        Expression::NumberLiteral(number_literal) => {
            let normalized_literal = number_literal.replace('_', "");
            let parsed_number = normalized_literal
                .parse::<f64>()
                .map_err(|_| WorkflowRuntimeError::InvalidNumberLiteral {
                    literal: number_literal.clone(),
                    context: evaluation_context.to_owned(),
                })?;

            let number_value = serde_json::Number::from_f64(parsed_number).ok_or_else(|| WorkflowRuntimeError::InvalidNumberLiteral {
                literal: normalized_literal,
                context: evaluation_context.to_owned(),
            })?;

            Ok(Value::Number(number_value))
        }
        Expression::BooleanLiteral(boolean_value) => Ok(Value::Bool(*boolean_value)),
        Expression::NullLiteral => Ok(Value::Null),
        Expression::Reference(reference) => evaluate_reference(reference, execution_scope, evaluation_context),
        Expression::ArrayLiteral(array_values) => {
            let mut rendered_values = Vec::new();

            for array_value in array_values {
                rendered_values.push(evaluate_expression(array_value, execution_scope, evaluation_context)?);
            }

            Ok(Value::Array(rendered_values))
        }
        Expression::ObjectLiteral(object_fields) => {
            let mut rendered_object = Map::new();

            for object_field in object_fields {
                let object_field_value = evaluate_expression(&object_field.value, execution_scope, evaluation_context)?;
                rendered_object.insert(object_field.name.clone(), object_field_value);
            }

            Ok(Value::Object(rendered_object))
        }
        Expression::FunctionCall(function_call) => {
            let function_name = reference_root_to_string(&function_call.callee.root);

            Err(WorkflowRuntimeError::UnsupportedFunctionCall {
                function_name,
                context: evaluation_context.to_owned(),
            })
        }
    }
}

fn evaluate_reference(
    reference: &Reference,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    match &reference.root {
        ReferenceRoot::Keyword(ReferenceKeyword::Agent) => evaluate_agent_reference(reference, execution_scope, evaluation_context),
        ReferenceRoot::Keyword(ReferenceKeyword::Input) => {
            apply_reference_accesses(reference, execution_scope.input_values, 0, evaluation_context)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Secrets) => {
            apply_reference_accesses(reference, execution_scope.secret_values, 0, evaluation_context)
        }
        ReferenceRoot::Keyword(ReferenceKeyword::Tool) => Err(WorkflowRuntimeError::UnsupportedToolKeywordReference {
            context: evaluation_context.to_owned(),
        }),
        ReferenceRoot::Identifier(identifier) => Err(WorkflowRuntimeError::UnknownReferenceIdentifier {
            identifier: identifier.clone(),
            context: evaluation_context.to_owned(),
        }),
    }
}

fn evaluate_agent_reference(
    reference: &Reference,
    execution_scope: &ExecutionScope<'_>,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    let Some(first_access) = reference.accesses.first() else {
        return Err(WorkflowRuntimeError::InvalidReferencePath {
            reference_path: reference_to_string(reference),
            field_name: "<missing-agent-name>".to_owned(),
            context: evaluation_context.to_owned(),
        });
    };

    let starting_value = execution_scope
        .agent_outputs_by_name
        .get(first_access.field.as_str())
        .ok_or_else(|| WorkflowRuntimeError::InvalidReferencePath {
            reference_path: reference_to_string(reference),
            field_name: first_access.field.clone(),
            context: evaluation_context.to_owned(),
        })?;

    if reference.accesses.len() == 1 {
        return Ok(starting_value.clone());
    }

    apply_reference_accesses(reference, starting_value, 1, evaluation_context)
}

fn apply_reference_accesses(
    reference: &Reference,
    starting_value: &Value,
    start_index: usize,
    evaluation_context: &str,
) -> Result<Value, WorkflowRuntimeError> {
    let mut current_value = starting_value.clone();

    for reference_access in reference.accesses.iter().skip(start_index) {
        match &current_value {
            Value::Object(object_value) => {
                if let Some(next_value) = object_value.get(reference_access.field.as_str()) {
                    current_value = next_value.clone();
                } else if reference_access.optional {
                    return Ok(Value::Null);
                } else {
                    return Err(WorkflowRuntimeError::InvalidReferencePath {
                        reference_path: reference_to_string(reference),
                        field_name: reference_access.field.clone(),
                        context: evaluation_context.to_owned(),
                    });
                }
            }
            _ if reference_access.optional => {
                return Ok(Value::Null);
            }
            _ => {
                return Err(WorkflowRuntimeError::InvalidReferencePath {
                    reference_path: reference_to_string(reference),
                    field_name: reference_access.field.clone(),
                    context: evaluation_context.to_owned(),
                });
            }
        }
    }

    Ok(current_value)
}

fn render_value_as_text(value: &Value) -> String {
    match value {
        Value::String(string_value) => string_value.clone(),
        Value::Null => "null".to_owned(),
        Value::Bool(boolean_value) => boolean_value.to_string(),
        Value::Number(number_value) => number_value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn reference_root_to_string(reference_root: &ReferenceRoot) -> String {
    match reference_root {
        ReferenceRoot::Keyword(reference_keyword) => reference_keyword.as_str().to_owned(),
        ReferenceRoot::Identifier(identifier) => identifier.clone(),
    }
}

fn reference_to_string(reference: &Reference) -> String {
    let mut rendered_reference = reference_root_to_string(&reference.root);

    for reference_access in &reference.accesses {
        let access_operator = if reference_access.optional { "?." } else { "." };

        rendered_reference.push_str(access_operator);
        rendered_reference.push_str(reference_access.field.as_str());
    }

    rendered_reference
}

fn validate_value_against_type_expression(
    candidate_value: &Value,
    type_expression: &TypeExpression,
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    match type_expression {
        TypeExpression::String => validate_primitive_match(candidate_value, current_path, "string", Value::is_string),
        TypeExpression::Number | TypeExpression::Float => {
            validate_primitive_match(candidate_value, current_path, "number", Value::is_number)
        }
        TypeExpression::Boolean => validate_primitive_match(candidate_value, current_path, "boolean", Value::is_boolean),
        TypeExpression::Null => validate_primitive_match(candidate_value, current_path, "null", Value::is_null),
        TypeExpression::SchemaReference(schema_name) => {
            validate_schema_reference_type(candidate_value, schema_name, workflow, current_path)
        }
        TypeExpression::StringEnum(enum_value) => validate_string_enum_type(candidate_value, enum_value, current_path),
        TypeExpression::Array { item_type, fixed_length } => {
            validate_array_type(candidate_value, item_type, *fixed_length, workflow, current_path)
        }
        TypeExpression::Tuple(tuple_types) => validate_tuple_type(candidate_value, tuple_types, workflow, current_path),
        TypeExpression::Object(object_fields) => validate_object_fields(candidate_value, object_fields.as_slice(), workflow, current_path),
        TypeExpression::Union(union_types) => validate_union_type(candidate_value, union_types, workflow, current_path),
    }
}

fn validate_primitive_match(
    candidate_value: &Value,
    current_path: &str,
    expected_type: &str,
    matcher: fn(&Value) -> bool,
) -> Result<(), String> {
    if matcher(candidate_value) {
        Ok(())
    } else {
        Err(format!(
            "expected {expected_type} at {current_path}, got {}",
            describe_json_value(candidate_value)
        ))
    }
}

fn validate_schema_reference_type(
    candidate_value: &Value,
    schema_name: &str,
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let schema_declaration = workflow
        .find_schema(schema_name)
        .ok_or_else(|| format!("unknown schema reference '{schema_name}' at {current_path}"))?;

    validate_object_fields(candidate_value, schema_declaration.fields.as_slice(), workflow, current_path)
}

fn validate_string_enum_type(candidate_value: &Value, enum_value: &str, current_path: &str) -> Result<(), String> {
    match candidate_value.as_str() {
        Some(candidate_string) if candidate_string == enum_value => Ok(()),
        Some(candidate_string) => Err(format!("expected '{enum_value}' at {current_path}, got '{candidate_string}'")),
        None => Err(format!(
            "expected string enum '{enum_value}' at {current_path}, got {}",
            describe_json_value(candidate_value)
        )),
    }
}

fn validate_array_type(
    candidate_value: &Value,
    item_type: &TypeExpression,
    fixed_length: Option<u64>,
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let candidate_array = candidate_value
        .as_array()
        .ok_or_else(|| format!("expected array at {current_path}, got {}", describe_json_value(candidate_value)))?;

    if let Some(expected_length) = fixed_length {
        let expected_length_as_usize =
            usize::try_from(expected_length).map_err(|_| format!("array length is too large at {current_path}"))?;

        if candidate_array.len() != expected_length_as_usize {
            return Err(format!(
                "expected array length {expected_length_as_usize} at {current_path}, got {}",
                candidate_array.len()
            ));
        }
    }

    for (index, array_item) in candidate_array.iter().enumerate() {
        let item_path = format!("{current_path}[{index}]");
        validate_value_against_type_expression(array_item, item_type, workflow, item_path.as_str())?;
    }

    Ok(())
}

fn validate_tuple_type(
    candidate_value: &Value,
    tuple_types: &[TypeExpression],
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let candidate_array = candidate_value.as_array().ok_or_else(|| {
        format!(
            "expected tuple array at {current_path}, got {}",
            describe_json_value(candidate_value)
        )
    })?;

    if candidate_array.len() != tuple_types.len() {
        return Err(format!(
            "expected tuple length {} at {current_path}, got {}",
            tuple_types.len(),
            candidate_array.len()
        ));
    }

    for (index, (array_item, tuple_item_type)) in candidate_array.iter().zip(tuple_types).enumerate() {
        let item_path = format!("{current_path}[{index}]");
        validate_value_against_type_expression(array_item, tuple_item_type, workflow, item_path.as_str())?;
    }

    Ok(())
}

fn validate_union_type(
    candidate_value: &Value,
    union_types: &[TypeExpression],
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let mut mismatch_messages = Vec::new();

    for union_type in union_types {
        match validate_value_against_type_expression(candidate_value, union_type, workflow, current_path) {
            Ok(()) => {
                return Ok(());
            }
            Err(mismatch_message) => {
                mismatch_messages.push(mismatch_message);
            }
        }
    }

    Err(format!(
        "value at {current_path} does not match any union type: {}",
        mismatch_messages.join(" | ")
    ))
}

fn validate_object_fields(
    candidate_value: &Value,
    object_fields: &[TypedField],
    workflow: &Workflow,
    current_path: &str,
) -> Result<(), String> {
    let candidate_object = candidate_value
        .as_object()
        .ok_or_else(|| format!("expected object at {current_path}, got {}", describe_json_value(candidate_value)))?;

    for object_field in object_fields {
        let object_field_value = candidate_object
            .get(object_field.name.as_str())
            .ok_or_else(|| format!("missing required field '{}' at {current_path}", object_field.name))?;

        let field_path = format!("{current_path}.{}", object_field.name);

        validate_value_against_type_expression(object_field_value, &object_field.field_type, workflow, field_path.as_str())?;
    }

    Ok(())
}

fn describe_json_value(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn determine_agent_execution_order(workflow: &Workflow) -> Result<Vec<String>, WorkflowRuntimeError> {
    let mut dependency_graph = DiGraph::<String, ()>::new();
    let mut node_index_by_agent_name = HashMap::<String, NodeIndex>::new();

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let node_index = dependency_graph.add_node(agent_declaration.name.clone());
        node_index_by_agent_name.insert(agent_declaration.name.clone(), node_index);
    }

    for declaration in workflow.declarations() {
        let Declaration::Agent(agent_declaration) = declaration else {
            continue;
        };

        let source_node_index = node_index_by_agent_name
            .get(agent_declaration.name.as_str())
            .copied()
            .expect("agent nodes should be inserted before dependency edges");
        let mut referenced_agent_names = HashSet::<String>::new();

        if let Some(agent_for_loop) = &agent_declaration.for_loop {
            collect_agent_dependencies_from_expression(&agent_for_loop.iterable, &mut referenced_agent_names);
        }

        for agent_property in &agent_declaration.properties {
            match agent_property {
                AgentProperty::Model(model_expression)
                | AgentProperty::Prompt(model_expression)
                | AgentProperty::Context(model_expression)
                | AgentProperty::Inference(model_expression)
                | AgentProperty::Tools(model_expression)
                | AgentProperty::Custom {
                    name: _,
                    value: model_expression,
                } => {
                    collect_agent_dependencies_from_expression(model_expression, &mut referenced_agent_names);
                }
                AgentProperty::Output(_) => {}
            }
        }

        for referenced_agent_name in referenced_agent_names {
            let Some(target_node_index) = node_index_by_agent_name.get(referenced_agent_name.as_str()).copied() else {
                continue;
            };

            if dependency_graph.find_edge(target_node_index, source_node_index).is_none() {
                dependency_graph.add_edge(target_node_index, source_node_index, ());
            }
        }
    }

    if toposort(&dependency_graph, None).is_err() {
        let mut cycle_agent_names = Vec::<String>::new();

        for strongly_connected_component in kosaraju_scc(&dependency_graph) {
            let has_cycle = if strongly_connected_component.len() > 1 {
                true
            } else {
                let node_index = strongly_connected_component[0];

                dependency_graph.find_edge(node_index, node_index).is_some()
            };

            if !has_cycle {
                continue;
            }

            for node_index in strongly_connected_component {
                cycle_agent_names.push(dependency_graph[node_index].clone());
            }
        }

        cycle_agent_names.sort();
        cycle_agent_names.dedup();

        return Err(WorkflowRuntimeError::DependencyCycle {
            agent_names: cycle_agent_names,
        });
    }

    let sorted_node_indices =
        toposort(&dependency_graph, None).expect("toposort should succeed after cycle pre-check in the same function");

    Ok(sorted_node_indices
        .into_iter()
        .map(|node_index| dependency_graph[node_index].clone())
        .collect())
}

fn collect_agent_dependencies_from_expression(expression: &Expression, referenced_agent_names: &mut HashSet<String>) {
    match expression {
        Expression::Reference(reference) => {
            collect_agent_dependency_from_reference(reference, referenced_agent_names);
        }
        Expression::FunctionCall(function_call) => {
            collect_agent_dependency_from_reference(&function_call.callee, referenced_agent_names);

            for call_argument in &function_call.arguments {
                match call_argument {
                    CallArgument::Positional(argument_expression) => {
                        collect_agent_dependencies_from_expression(argument_expression, referenced_agent_names);
                    }
                    CallArgument::Named(named_argument) => {
                        collect_agent_dependencies_from_expression(&named_argument.value, referenced_agent_names);
                    }
                }
            }
        }
        Expression::ArrayLiteral(array_values) => {
            for array_value in array_values {
                collect_agent_dependencies_from_expression(array_value, referenced_agent_names);
            }
        }
        Expression::ObjectLiteral(object_fields) => {
            for object_field in object_fields {
                collect_agent_dependencies_from_expression(&object_field.value, referenced_agent_names);
            }
        }
        Expression::StringTemplate(string_template) => {
            for string_template_part in &string_template.parts {
                if let StringTemplatePart::Interpolation(interpolation_expression) = string_template_part {
                    collect_agent_dependencies_from_expression(interpolation_expression, referenced_agent_names);
                }
            }
        }
        Expression::StringLiteral(_) | Expression::NumberLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral => {}
    }
}

fn collect_agent_dependency_from_reference(reference: &Reference, referenced_agent_names: &mut HashSet<String>) {
    if reference.root.keyword() != Some(ReferenceKeyword::Agent) {
        return;
    }

    let Some(first_access) = reference.accesses.first() else {
        return;
    };

    referenced_agent_names.insert(first_access.field.clone());
}

#[cfg(test)]
mod tests {
    use super::{DynamicProvider, ScriptedProviderFactory, WorkflowProviderFactory, WorkflowRuntime, WorkflowRuntimeError};
    use async_trait::async_trait;
    use engine_ai_agent::{Context, Message, Provider, ProviderError, ProviderResponse, StopReason, ToolCall, ToolDefinition};
    use serde_json::{json, Map, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn executes_existing_minimum_workflow_with_scripted_provider() {
        let workflow_source = include_str!("../../workflows/minimum.ai");
        let mut outputs_by_agent_name = HashMap::<String, Value>::new();

        outputs_by_agent_name.insert("greeting".to_owned(), Value::String("Hello from scripted runtime".to_owned()));

        let workflow_runtime = WorkflowRuntime::new(ScriptedProviderFactory::new(outputs_by_agent_name));

        let execution_result = workflow_runtime
            .execute_source(workflow_source, json!({}), json!({}))
            .await
            .expect("minimum workflow should execute successfully");

        assert_eq!(
            execution_result.output,
            json!({
                "greeting": "Hello from scripted runtime"
            })
        );
    }

    #[tokio::test]
    async fn resolves_dependencies_and_interpolates_prompt_references() {
        let workflow_source = r#"
            provider scripted {
                driver: "scripted"
                models: ["mock-model"]
            }

            input {
                subject: string
            }

            agent first {
                model: scripted("mock-model")
                prompt: "First prompt: {{ input.subject }}"
                output: {
                    summary: string
                }
            }

            agent second {
                model: scripted("mock-model")
                prompt: "Second prompt: {{ agent.first.summary }}"
                output: string
            }

            output {
                first_summary: agent.first.summary
                second_text: agent.second
            }
        "#;

        let mut outputs_by_agent_name = HashMap::<String, Value>::new();

        outputs_by_agent_name.insert(
            "first".to_owned(),
            json!({
                "summary": "summary-from-first"
            }),
        );
        outputs_by_agent_name.insert("second".to_owned(), Value::String("answer-from-second".to_owned()));

        let recorded_prompts = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let workflow_runtime = WorkflowRuntime::new(PromptRecordingProviderFactory::new(
            outputs_by_agent_name,
            Arc::clone(&recorded_prompts),
        ));

        let execution_result = workflow_runtime
            .execute_source(
                workflow_source,
                json!({
                    "subject": "engine-ai"
                }),
                json!({}),
            )
            .await
            .expect("workflow should execute successfully");

        assert_eq!(
            execution_result.output,
            json!({
                "first_summary": "summary-from-first",
                "second_text": "answer-from-second"
            })
        );

        let recorded_prompts = recorded_prompts
            .lock()
            .expect("prompt recorder mutex should not be poisoned")
            .clone();

        assert_eq!(recorded_prompts.len(), 2);
        assert_eq!(recorded_prompts[0], ("first".to_owned(), "First prompt: engine-ai".to_owned()));
        assert_eq!(
            recorded_prompts[1],
            ("second".to_owned(), "Second prompt: summary-from-first".to_owned())
        );
    }

    #[tokio::test]
    async fn reports_agent_output_type_mismatch() {
        let workflow_source = r#"
            provider scripted {
                driver: "scripted"
                models: ["mock-model"]
            }

            agent first {
                model: scripted("mock-model")
                prompt: "irrelevant"
                output: {
                    score: number
                }
            }

            output {
                result: agent.first
            }
        "#;

        let mut outputs_by_agent_name = HashMap::<String, Value>::new();

        outputs_by_agent_name.insert("first".to_owned(), Value::String("this should have been an object".to_owned()));

        let workflow_runtime = WorkflowRuntime::new(ScriptedProviderFactory::new(outputs_by_agent_name));

        let execution_error = workflow_runtime
            .execute_source(workflow_source, json!({}), json!({}))
            .await
            .expect_err("runtime should reject mismatched output type");

        assert!(matches!(
            execution_error,
            WorkflowRuntimeError::AgentOutputTypeMismatch {
                agent_name,
                message: _
            } if agent_name == "first"
        ));
    }

    #[derive(Debug, Clone)]
    struct PromptRecordingProviderFactory {
        outputs_by_agent_name: HashMap<String, Value>,
        recorded_prompts: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl PromptRecordingProviderFactory {
        fn new(outputs_by_agent_name: HashMap<String, Value>, recorded_prompts: Arc<Mutex<Vec<(String, String)>>>) -> Self {
            Self {
                outputs_by_agent_name,
                recorded_prompts,
            }
        }
    }

    impl WorkflowProviderFactory for PromptRecordingProviderFactory {
        fn build_provider(
            &self,
            agent_name: &str,
            _provider_name: &str,
            _provider_settings: &Map<String, Value>,
            _model_name: &str,
        ) -> Result<DynamicProvider, WorkflowRuntimeError> {
            let output_value =
                self.outputs_by_agent_name
                    .get(agent_name)
                    .cloned()
                    .ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
                        message: format!("missing scripted output for '{agent_name}'"),
                    })?;

            Ok(DynamicProvider::new(PromptRecordingProvider {
                agent_name: agent_name.to_owned(),
                output_value,
                recorded_prompts: Arc::clone(&self.recorded_prompts),
            }))
        }
    }

    #[derive(Debug, Clone)]
    struct PromptRecordingProvider {
        agent_name: String,
        output_value: Value,
        recorded_prompts: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait]
    impl Provider for PromptRecordingProvider {
        async fn generate(
            &self,
            context: &Context,
            _tools: &[ToolDefinition],
            _config: &engine_ai_agent::AgentConfig,
        ) -> Result<ProviderResponse, ProviderError> {
            let prompt_text = context
                .messages
                .iter()
                .rev()
                .find_map(|message| match message {
                    Message::User { content } => Some(content.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            self.recorded_prompts
                .lock()
                .expect("prompt recorder mutex should not be poisoned")
                .push((self.agent_name.clone(), prompt_text));

            let finalize_tool_call = ToolCall {
                id: format!("{}-finalize", self.agent_name),
                name: "finalize".to_owned(),
                arguments: json!({
                    "output": {
                        "type": "success",
                        "answer": self.output_value.clone()
                    }
                }),
            };

            Ok(ProviderResponse {
                tool_calls: vec![finalize_tool_call],
                text: None,
                stop_reason: StopReason::ToolCalls,
                usage: None,
            })
        }
    }
}
