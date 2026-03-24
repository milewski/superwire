use crate::ast::{
    AccessOperator, AgentDeclaration, AgentProperty, CompactArgument, CompactExpression, Expression, FunctionExpression, InferenceProperty,
    PromptValue, ProviderDeclaration, ProviderProperty, ReferenceExpression, ReferenceRoot, StringFragment, StringTemplate, ToolUsage,
    TypeExpression, TypeField, Workflow,
};
use crate::compiler::graph::DependencyGraph;
use crate::compiler::schema::build_object_schema;
use crate::compiler::template::TemplateDocument;
use crate::compiler::types::{is_nullable_type, property_type, remove_null_from_type, resolve_type, ReferenceType};
use crate::compiler::{CompiledAgent, CompiledProvider, CompiledWorkflow, ProviderDriver};
use crate::error::WorkflowError;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub fn compile_workflow(workflow: Workflow, base_path: PathBuf) -> Result<CompiledWorkflow, WorkflowError> {
    ensure_unique_field_names(&workflow.input_fields, "input")?;
    ensure_unique_field_names(&workflow.secret_fields, "secrets")?;

    let named_schemas = build_named_schema_map(&workflow.schemas)?;
    ensure_types_are_resolvable(&workflow.input_fields, &named_schemas, "input")?;
    ensure_types_are_resolvable(&workflow.secret_fields, &named_schemas, "secrets")?;

    let providers = build_provider_map(&workflow.providers)?;
    let normalized_agents = workflow
        .agents
        .iter()
        .map(normalize_agent_declaration)
        .collect::<Result<Vec<_>, _>>()?;
    let agent_types = build_agent_type_map(&normalized_agents, &named_schemas)?;
    let (compiled_agents, dependencies_by_agent) = build_compiled_agents(
        normalized_agents,
        &agent_types,
        &base_path,
        &workflow.input_fields,
        &workflow.secret_fields,
        &named_schemas,
        &providers,
    )?;

    let dependency_graph = DependencyGraph::new(&dependencies_by_agent)?;
    validate_output_fields(
        &workflow.output_fields,
        &agent_types,
        &workflow.input_fields,
        &workflow.secret_fields,
        &named_schemas,
    )?;

    let input_schema = if workflow.input_fields.is_empty() {
        None
    } else {
        Some(build_object_schema(&workflow.input_fields, &named_schemas)?)
    };
    let secret_schema = if workflow.secret_fields.is_empty() {
        None
    } else {
        Some(build_object_schema(&workflow.secret_fields, &named_schemas)?)
    };

    Ok(CompiledWorkflow {
        agents: compiled_agents,
        base_path,
        dependency_graph,
        input_fields: workflow.input_fields,
        input_schema,
        output_fields: workflow.output_fields,
        providers,
        schemas: named_schemas,
        secret_fields: workflow.secret_fields,
        secret_schema,
    })
}

fn build_compiled_agents(
    normalized_agents: Vec<NormalizedAgent>,
    agent_types: &BTreeMap<String, ReferenceType>,
    base_path: &Path,
    input_fields: &[TypeField],
    secret_fields: &[TypeField],
    named_schemas: &BTreeMap<String, TypeExpression>,
    providers: &BTreeMap<String, CompiledProvider>,
) -> Result<CompiledAgentBatch, WorkflowError> {
    let mut compiled_agents = Vec::with_capacity(normalized_agents.len());
    let mut dependencies_by_agent = BTreeMap::new();

    for normalized_agent in normalized_agents {
        ensure_provider_model_exists(&normalized_agent.model.provider_name, &normalized_agent.model.model_name, providers)?;
        ensure_agent_output_type_is_resolvable(&normalized_agent, named_schemas)?;

        let loop_locals = build_loop_locals(&normalized_agent, agent_types, named_schemas, input_fields, secret_fields)?;
        let validation_context = ValidationContext {
            agent_types,
            input_fields,
            local_variables: &loop_locals,
            schemas: named_schemas,
            secret_fields,
        };
        let dependencies = build_agent_dependencies(&normalized_agent, base_path, &validation_context, named_schemas)?;

        dependencies_by_agent.insert(normalized_agent.name.clone(), dependencies.clone());
        compiled_agents.push(CompiledAgent {
            context: normalized_agent.context,
            dependencies,
            for_loop: normalized_agent.for_loop,
            inference: normalized_agent.inference,
            model: normalized_agent.model,
            name: normalized_agent.name,
            output_type: normalized_agent.output_type,
            prompt: normalized_agent.prompt,
            tools: normalized_agent.tools,
        });
    }

    Ok((compiled_agents, dependencies_by_agent))
}

fn ensure_agent_output_type_is_resolvable(
    agent: &NormalizedAgent,
    named_schemas: &BTreeMap<String, TypeExpression>,
) -> Result<(), WorkflowError> {
    ensure_types_are_resolvable(
        &[TypeField {
            name: agent.name.clone(),
            value_type: agent.output_type.clone(),
            description: None,
        }],
        named_schemas,
        &format!("agent '{}' output", agent.name),
    )
}

fn build_agent_dependencies(
    agent: &NormalizedAgent,
    base_path: &Path,
    validation_context: &ValidationContext<'_>,
    named_schemas: &BTreeMap<String, TypeExpression>,
) -> Result<BTreeSet<String>, WorkflowError> {
    let mut dependencies = BTreeSet::new();
    dependencies.extend(validate_prompt(
        &agent.prompt,
        base_path,
        validation_context,
        SecretPolicy::Deny { location: "prompt" },
    )?);

    if let Some(context_expression) = &agent.context {
        dependencies.extend(validate_agent_context_expression(context_expression, validation_context)?);
    }

    dependencies.extend(validate_tool_usages(&agent.tools, validation_context)?);
    validate_inference_properties(&agent.inference, &agent.name)?;
    validate_loop_source(agent, validation_context, named_schemas)?;

    Ok(dependencies)
}

fn validate_loop_source(
    agent: &NormalizedAgent,
    validation_context: &ValidationContext<'_>,
    named_schemas: &BTreeMap<String, TypeExpression>,
) -> Result<(), WorkflowError> {
    let Some(for_loop) = &agent.for_loop else {
        return Ok(());
    };
    let loop_source_type = analyze_expression(&for_loop.source, validation_context, SecretPolicy::Deny { location: "agent loop" })?
        .value_type
        .ok_or_else(|| WorkflowError::validation(format!("agent '{}' loop source must resolve to an array", agent.name)))?;

    if !matches!(
        resolve_type(&loop_source_type, named_schemas)?,
        TypeExpression::Array(_) | TypeExpression::FixedArray { .. }
    ) {
        return Err(WorkflowError::validation(format!(
            "agent '{}' loop source must evaluate to an array",
            agent.name
        )));
    }

    Ok(())
}

fn validate_output_fields(
    output_fields: &[crate::ast::ObjectField],
    agent_types: &BTreeMap<String, ReferenceType>,
    input_fields: &[TypeField],
    secret_fields: &[TypeField],
    named_schemas: &BTreeMap<String, TypeExpression>,
) -> Result<(), WorkflowError> {
    let empty_locals = BTreeMap::new();
    let output_validation_context = ValidationContext {
        agent_types,
        input_fields,
        local_variables: &empty_locals,
        schemas: named_schemas,
        secret_fields,
    };

    for output_field in output_fields {
        analyze_expression(
            &output_field.value,
            &output_validation_context,
            SecretPolicy::Deny { location: "output" },
        )?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct NormalizedAgent {
    context: Option<Expression>,
    for_loop: Option<crate::ast::ForLoopBinding>,
    inference: Vec<InferenceProperty>,
    model: crate::ast::ModelSelector,
    name: String,
    output_type: TypeExpression,
    prompt: PromptValue,
    tools: Vec<ToolUsage>,
}

#[derive(Debug, Clone)]
struct ValidationContext<'a> {
    agent_types: &'a BTreeMap<String, ReferenceType>,
    input_fields: &'a [TypeField],
    local_variables: &'a BTreeMap<String, TypeExpression>,
    schemas: &'a BTreeMap<String, TypeExpression>,
    secret_fields: &'a [TypeField],
}

#[derive(Debug, Clone)]
struct ExpressionAnalysis {
    dependencies: BTreeSet<String>,
    value_type: Option<TypeExpression>,
}

#[derive(Debug, Clone, Copy)]
enum SecretPolicy {
    Allow,
    Deny { location: &'static str },
}

type AgentDependencyMap = BTreeMap<String, BTreeSet<String>>;
type CompiledAgentBatch = (Vec<CompiledAgent>, AgentDependencyMap);

fn build_named_schema_map(schemas: &[(String, Vec<TypeField>)]) -> Result<BTreeMap<String, TypeExpression>, WorkflowError> {
    let mut named_schemas = BTreeMap::new();

    for (schema_name, schema_fields) in schemas {
        if named_schemas.contains_key(schema_name) {
            return Err(WorkflowError::validation(format!("duplicate schema declaration '{schema_name}'")));
        }

        ensure_unique_field_names(schema_fields, &format!("schema '{schema_name}'"))?;
        named_schemas.insert(schema_name.clone(), TypeExpression::Object(schema_fields.clone()));
    }

    for (schema_name, schema_type) in &named_schemas {
        resolve_type(schema_type, &named_schemas)
            .map_err(|error| WorkflowError::validation(format!("failed validating schema '{schema_name}': {error}")))?;
    }

    Ok(named_schemas)
}

fn build_provider_map(providers: &[ProviderDeclaration]) -> Result<BTreeMap<String, CompiledProvider>, WorkflowError> {
    let mut compiled_providers = BTreeMap::new();

    for provider in providers {
        if compiled_providers.contains_key(&provider.name) {
            return Err(WorkflowError::validation(format!(
                "duplicate provider declaration '{}'",
                provider.name
            )));
        }

        let mut driver = None;
        let mut endpoint = None;
        let mut api_key_secret_name = None;
        let mut models = None;

        for property in &provider.properties {
            match property {
                ProviderProperty::ApiKey(expression) => {
                    if api_key_secret_name.is_some() {
                        return Err(WorkflowError::validation(format!(
                            "provider '{}' declares 'api_key' more than once",
                            provider.name
                        )));
                    }

                    api_key_secret_name = Some(extract_secret_name(expression)?);
                }
                ProviderProperty::Driver(driver_value) => {
                    if driver.is_some() {
                        return Err(WorkflowError::validation(format!(
                            "provider '{}' declares 'driver' more than once",
                            provider.name
                        )));
                    }

                    driver = Some(parse_provider_driver(driver_value)?);
                }
                ProviderProperty::Endpoint(endpoint_value) => {
                    if endpoint.is_some() {
                        return Err(WorkflowError::validation(format!(
                            "provider '{}' declares 'endpoint' more than once",
                            provider.name
                        )));
                    }

                    endpoint = Some(endpoint_value.clone());
                }
                ProviderProperty::Models(model_values) => {
                    if models.is_some() {
                        return Err(WorkflowError::validation(format!(
                            "provider '{}' declares 'models' more than once",
                            provider.name
                        )));
                    }

                    models = Some(model_values.clone());
                }
            }
        }

        compiled_providers.insert(
            provider.name.clone(),
            CompiledProvider {
                api_key_secret_name,
                driver: driver.ok_or_else(|| WorkflowError::validation(format!("provider '{}' is missing 'driver'", provider.name)))?,
                endpoint,
                models: models.ok_or_else(|| WorkflowError::validation(format!("provider '{}' is missing 'models'", provider.name)))?,
                name: provider.name.clone(),
            },
        );
    }

    Ok(compiled_providers)
}

fn normalize_agent_declaration(agent: &AgentDeclaration) -> Result<NormalizedAgent, WorkflowError> {
    let mut context = None;
    let mut inference = None;
    let mut model = None;
    let mut output_type = None;
    let mut prompt = None;
    let mut tools = None;

    for property in &agent.properties {
        match property {
            AgentProperty::Context(expression) => {
                if context.is_some() {
                    return Err(WorkflowError::validation(format!(
                        "agent '{}' declares 'context' more than once",
                        agent.name
                    )));
                }

                context = Some(expression.clone());
            }
            AgentProperty::Inference(inference_properties) => {
                if inference.is_some() {
                    return Err(WorkflowError::validation(format!(
                        "agent '{}' declares 'inference' more than once",
                        agent.name
                    )));
                }

                inference = Some(inference_properties.clone());
            }
            AgentProperty::Model(model_selector) => {
                if model.is_some() {
                    return Err(WorkflowError::validation(format!(
                        "agent '{}' declares 'model' more than once",
                        agent.name
                    )));
                }

                model = Some(model_selector.clone());
            }
            AgentProperty::Output(type_expression) => {
                if output_type.is_some() {
                    return Err(WorkflowError::validation(format!(
                        "agent '{}' declares 'output' more than once",
                        agent.name
                    )));
                }

                output_type = Some(type_expression.clone());
            }
            AgentProperty::Prompt(prompt_value) => {
                if prompt.is_some() {
                    return Err(WorkflowError::validation(format!(
                        "agent '{}' declares 'prompt' more than once",
                        agent.name
                    )));
                }

                prompt = Some(prompt_value.clone());
            }
            AgentProperty::Tools(tool_usages) => {
                if tools.is_some() {
                    return Err(WorkflowError::validation(format!(
                        "agent '{}' declares 'tools' more than once",
                        agent.name
                    )));
                }

                tools = Some(tool_usages.clone());
            }
        }
    }

    Ok(NormalizedAgent {
        context,
        for_loop: agent.for_loop.clone(),
        inference: inference.unwrap_or_default(),
        model: model.ok_or_else(|| WorkflowError::validation(format!("agent '{}' is missing 'model'", agent.name)))?,
        name: agent.name.clone(),
        output_type: output_type.ok_or_else(|| WorkflowError::validation(format!("agent '{}' is missing 'output'", agent.name)))?,
        prompt: prompt.ok_or_else(|| WorkflowError::validation(format!("agent '{}' is missing 'prompt'", agent.name)))?,
        tools: tools.unwrap_or_default(),
    })
}

fn build_agent_type_map(
    agents: &[NormalizedAgent],
    schemas: &BTreeMap<String, TypeExpression>,
) -> Result<BTreeMap<String, ReferenceType>, WorkflowError> {
    let mut agent_types = BTreeMap::new();

    for agent in agents {
        if agent_types.contains_key(&agent.name) {
            return Err(WorkflowError::validation(format!("duplicate agent declaration '{}'", agent.name)));
        }

        agent_types.insert(
            agent.name.clone(),
            ReferenceType {
                is_collection: agent.for_loop.is_some(),
                value_type: resolve_type(&agent.output_type, schemas)?,
            },
        );
    }

    Ok(agent_types)
}

fn build_loop_locals(
    agent: &NormalizedAgent,
    agent_types: &BTreeMap<String, ReferenceType>,
    schemas: &BTreeMap<String, TypeExpression>,
    input_fields: &[TypeField],
    secret_fields: &[TypeField],
) -> Result<BTreeMap<String, TypeExpression>, WorkflowError> {
    let mut local_variables = BTreeMap::new();

    let Some(for_loop) = &agent.for_loop else {
        return Ok(local_variables);
    };

    let expression_context = ValidationContext {
        agent_types,
        input_fields,
        local_variables: &BTreeMap::new(),
        schemas,
        secret_fields,
    };
    let source_type = analyze_expression(&for_loop.source, &expression_context, SecretPolicy::Deny { location: "agent loop" })?
        .value_type
        .ok_or_else(|| WorkflowError::validation(format!("agent '{}' loop source must yield an array", agent.name)))?;

    let item_type = match resolve_type(&source_type, schemas)? {
        TypeExpression::Array(item_type) => *item_type,
        TypeExpression::FixedArray { item_type, .. } => *item_type,
        _ => {
            return Err(WorkflowError::validation(format!(
                "agent '{}' loop source must yield an array",
                agent.name
            )));
        }
    };

    local_variables.insert(for_loop.item_name.clone(), item_type);
    Ok(local_variables)
}

fn ensure_types_are_resolvable(
    fields: &[TypeField],
    schemas: &BTreeMap<String, TypeExpression>,
    context_name: &str,
) -> Result<(), WorkflowError> {
    for field in fields {
        resolve_type(&field.value_type, schemas)
            .map_err(|error| WorkflowError::validation(format!("failed validating {context_name} field '{}': {error}", field.name)))?;
    }

    Ok(())
}

fn ensure_unique_field_names(fields: &[TypeField], context_name: &str) -> Result<(), WorkflowError> {
    let mut field_names = BTreeSet::new();

    for field in fields {
        if !field_names.insert(field.name.clone()) {
            return Err(WorkflowError::validation(format!(
                "duplicate field '{}' in {context_name}",
                field.name
            )));
        }
    }

    Ok(())
}

fn ensure_provider_model_exists(
    provider_name: &str,
    model_name: &str,
    providers: &BTreeMap<String, CompiledProvider>,
) -> Result<(), WorkflowError> {
    let provider = providers
        .get(provider_name)
        .ok_or_else(|| WorkflowError::validation(format!("unknown provider '{provider_name}'")))?;

    if !provider.models.iter().any(|candidate_model| candidate_model == model_name) {
        return Err(WorkflowError::validation(format!(
            "provider '{provider_name}' does not declare model '{model_name}'"
        )));
    }

    Ok(())
}

fn parse_provider_driver(driver_value: &str) -> Result<ProviderDriver, WorkflowError> {
    match driver_value {
        "ollama" => Ok(ProviderDriver::Ollama),
        "openai" => Ok(ProviderDriver::OpenAi),
        _ => Err(WorkflowError::validation(format!("unsupported provider driver '{driver_value}'"))),
    }
}

fn extract_secret_name(expression: &Expression) -> Result<String, WorkflowError> {
    match expression {
        Expression::Reference(ReferenceExpression {
            root: ReferenceRoot::Secrets(secret_name),
            path,
        }) if path.is_empty() => Ok(secret_name.clone()),
        _ => Err(WorkflowError::validation("provider 'api_key' must reference a declared secret")),
    }
}

fn validate_prompt(
    prompt: &PromptValue,
    base_path: &Path,
    context: &ValidationContext<'_>,
    secret_policy: SecretPolicy,
) -> Result<BTreeSet<String>, WorkflowError> {
    match prompt {
        PromptValue::Inline(template) => validate_string_template(template, context, secret_policy),
        PromptValue::Template { path, bindings } => {
            let template_document = TemplateDocument::load(base_path, path)?;
            let binding_names = bindings.iter().map(|binding| binding.name.clone()).collect::<BTreeSet<_>>();

            if binding_names.len() != bindings.len() {
                return Err(WorkflowError::validation(format!(
                    "template '{path}' contains duplicate binding keys"
                )));
            }

            if binding_names != template_document.placeholders {
                return Err(WorkflowError::validation(format!(
                    "template '{path}' bindings must exactly match placeholders in '{}'",
                    template_document.path.display()
                )));
            }

            let mut dependencies = BTreeSet::new();

            for binding in bindings {
                dependencies.extend(analyze_expression(&binding.value, context, secret_policy)?.dependencies);
            }

            Ok(dependencies)
        }
    }
}

fn validate_string_template(
    template: &StringTemplate,
    context: &ValidationContext<'_>,
    secret_policy: SecretPolicy,
) -> Result<BTreeSet<String>, WorkflowError> {
    let mut dependencies = BTreeSet::new();

    for fragment in &template.fragments {
        if let StringFragment::Expression(expression) = fragment {
            dependencies.extend(analyze_expression(expression, context, secret_policy)?.dependencies);
        }
    }

    Ok(dependencies)
}

fn validate_agent_context_expression(expression: &Expression, context: &ValidationContext<'_>) -> Result<BTreeSet<String>, WorkflowError> {
    match expression {
        Expression::Function(FunctionExpression::Context(reference)) => {
            let reference_type = resolve_reference_type(reference, context)?;

            if reference_type.is_collection {
                return Err(WorkflowError::validation("agent context cannot be sourced from a looped agent"));
            }

            Ok(analyze_reference_dependencies(reference))
        }
        Expression::Function(FunctionExpression::Compact(compact_expression)) => {
            validate_compact_expression(compact_expression, context, false)
        }
        _ => Err(WorkflowError::validation("agent context must use context(...) or compact(...)")),
    }
}

fn validate_compact_expression(
    compact_expression: &CompactExpression,
    context: &ValidationContext<'_>,
    allow_looped_source: bool,
) -> Result<BTreeSet<String>, WorkflowError> {
    let mut dependencies = BTreeSet::new();
    let mut agent_count = 0;
    let mut inference_count = 0;
    let mut model_count = 0;
    let mut prompt_count = 0;

    for argument in &compact_expression.arguments {
        match argument {
            CompactArgument::Agent(reference) => {
                agent_count += 1;
                let reference_type = resolve_reference_type(reference, context)?;

                if reference_type.is_collection && !allow_looped_source {
                    return Err(WorkflowError::validation("compact(...) cannot use a looped agent as context input"));
                }

                dependencies.extend(analyze_reference_dependencies(reference));
            }
            CompactArgument::Inference(inference_properties) => {
                inference_count += 1;
                validate_inference_properties(inference_properties, "compact")?;
            }
            CompactArgument::Model(model_selector) => {
                model_count += 1;

                if !context.agent_types.is_empty() {
                    // Provider/model existence is validated later at runtime execution boundaries.
                    let _ = model_selector;
                }
            }
            CompactArgument::Prompt(template) => {
                prompt_count += 1;
                dependencies.extend(validate_string_template(
                    template,
                    context,
                    SecretPolicy::Deny {
                        location: "compact prompt",
                    },
                )?);
            }
        }
    }

    if agent_count != 1 {
        return Err(WorkflowError::validation("compact(...) requires exactly one agent reference"));
    }

    if inference_count > 1 || model_count > 1 || prompt_count > 1 {
        return Err(WorkflowError::validation(
            "compact(...) accepts at most one each of agent, model, inference, and prompt arguments",
        ));
    }

    Ok(dependencies)
}

fn validate_tool_usages(tool_usages: &[ToolUsage], context: &ValidationContext<'_>) -> Result<BTreeSet<String>, WorkflowError> {
    let mut tool_names = BTreeSet::new();
    let mut dependencies = BTreeSet::new();

    for tool_usage in tool_usages {
        if !tool_names.insert(tool_usage.name.clone()) {
            return Err(WorkflowError::validation(format!(
                "tool '{}' is declared more than once in the same agent",
                tool_usage.name
            )));
        }

        let binding_names = tool_usage
            .arguments
            .iter()
            .map(|binding| binding.name.clone())
            .collect::<BTreeSet<_>>();

        if binding_names.len() != tool_usage.arguments.len() {
            return Err(WorkflowError::validation(format!(
                "tool '{}' contains duplicate argument names",
                tool_usage.name
            )));
        }

        for argument in &tool_usage.arguments {
            dependencies.extend(analyze_expression(&argument.value, context, SecretPolicy::Allow)?.dependencies);
        }
    }

    Ok(dependencies)
}

fn validate_inference_properties(inference_properties: &[InferenceProperty], owner_name: &str) -> Result<(), WorkflowError> {
    let mut property_names = BTreeSet::new();

    for inference_property in inference_properties {
        let property_name = match inference_property {
            InferenceProperty::FrequencyPenalty(_) => "frequency_penalty",
            InferenceProperty::MaxTokens(_) => "max_tokens",
            InferenceProperty::PresencePenalty(_) => "presence_penalty",
            InferenceProperty::RepeatPenalty(_) => "repeat_penalty",
            InferenceProperty::Seed(_) => "seed",
            InferenceProperty::StopSequences(_) => "stop_sequences",
            InferenceProperty::Temperature(_) => "temperature",
            InferenceProperty::TopK(_) => "top_k",
            InferenceProperty::TopP(_) => "top_p",
        };

        if !property_names.insert(property_name) {
            return Err(WorkflowError::validation(format!(
                "{owner_name} declares inference property '{property_name}' more than once"
            )));
        }
    }

    Ok(())
}

fn analyze_expression(
    expression: &Expression,
    context: &ValidationContext<'_>,
    secret_policy: SecretPolicy,
) -> Result<ExpressionAnalysis, WorkflowError> {
    match expression {
        Expression::Array(items) => {
            let mut dependencies = BTreeSet::new();
            let mut item_types = Vec::new();

            for item in items {
                let analysis = analyze_expression(item, context, secret_policy)?;
                dependencies.extend(analysis.dependencies);

                if let Some(item_type) = analysis.value_type {
                    item_types.push(item_type);
                }
            }

            let value_type = item_types.into_iter().reduce(|left_type, right_type| {
                if left_type == right_type {
                    left_type
                } else {
                    TypeExpression::Union(vec![left_type, right_type])
                }
            });

            Ok(ExpressionAnalysis {
                dependencies,
                value_type: Some(TypeExpression::Array(Box::new(value_type.unwrap_or(TypeExpression::Null)))),
            })
        }
        Expression::Boolean(_) => Ok(ExpressionAnalysis {
            dependencies: BTreeSet::new(),
            value_type: Some(TypeExpression::Primitive(crate::ast::PrimitiveType::Boolean)),
        }),
        Expression::Function(FunctionExpression::Compact(compact_expression)) => Ok(ExpressionAnalysis {
            dependencies: validate_compact_expression(compact_expression, context, true)?,
            value_type: Some(TypeExpression::Primitive(crate::ast::PrimitiveType::String)),
        }),
        Expression::Function(FunctionExpression::Context(reference)) => {
            let _reference_type = resolve_reference_type(reference, context)?;

            Ok(ExpressionAnalysis {
                dependencies: analyze_reference_dependencies(reference),
                value_type: None,
            })
        }
        Expression::Null => Ok(ExpressionAnalysis {
            dependencies: BTreeSet::new(),
            value_type: Some(TypeExpression::Null),
        }),
        Expression::Number(number_literal) => Ok(ExpressionAnalysis {
            dependencies: BTreeSet::new(),
            value_type: Some(if number_literal.contains('.') {
                TypeExpression::Primitive(crate::ast::PrimitiveType::Float)
            } else {
                TypeExpression::Primitive(crate::ast::PrimitiveType::Number)
            }),
        }),
        Expression::Object(fields) => {
            let mut dependencies = BTreeSet::new();
            let mut typed_fields = Vec::new();

            for field in fields {
                let analysis = analyze_expression(&field.value, context, secret_policy)?;
                dependencies.extend(analysis.dependencies);

                typed_fields.push(TypeField {
                    name: field.name.clone(),
                    value_type: analysis.value_type.unwrap_or(TypeExpression::Null),
                    description: None,
                });
            }

            Ok(ExpressionAnalysis {
                dependencies,
                value_type: Some(TypeExpression::Object(typed_fields)),
            })
        }
        Expression::Reference(reference) => {
            let reference_type = resolve_reference_type(reference, context)?;

            if let (ReferenceRoot::Secrets(_), SecretPolicy::Deny { location }) = (&reference.root, secret_policy) {
                return Err(WorkflowError::validation(format!("secrets cannot be referenced in {location}")));
            }

            let value_type = if reference_type.is_collection {
                TypeExpression::Array(Box::new(reference_type.value_type))
            } else {
                reference_type.value_type
            };

            Ok(ExpressionAnalysis {
                dependencies: analyze_reference_dependencies(reference),
                value_type: Some(value_type),
            })
        }
        Expression::String(template) => Ok(ExpressionAnalysis {
            dependencies: validate_string_template(template, context, secret_policy)?,
            value_type: Some(TypeExpression::Primitive(crate::ast::PrimitiveType::String)),
        }),
    }
}

fn resolve_reference_type(reference: &ReferenceExpression, context: &ValidationContext<'_>) -> Result<ReferenceType, WorkflowError> {
    let mut reference_type = match &reference.root {
        ReferenceRoot::Agent(agent_name) => context
            .agent_types
            .get(agent_name)
            .cloned()
            .ok_or_else(|| WorkflowError::validation(format!("unknown agent reference 'agent.{agent_name}'")))?,
        ReferenceRoot::Input(field_name) => ReferenceType {
            is_collection: false,
            value_type: lookup_field_type(context.input_fields, field_name, "input")?,
        },
        ReferenceRoot::Local(variable_name) => ReferenceType {
            is_collection: false,
            value_type: context
                .local_variables
                .get(variable_name)
                .cloned()
                .ok_or_else(|| WorkflowError::validation(format!("unknown local reference '{variable_name}'")))?,
        },
        ReferenceRoot::Secrets(secret_name) => ReferenceType {
            is_collection: false,
            value_type: lookup_field_type(context.secret_fields, secret_name, "secrets")?,
        },
    };

    for path_segment in &reference.path {
        if path_segment.operator == AccessOperator::Safe {
            if !is_nullable_type(&reference_type.value_type, context.schemas)? {
                return Err(WorkflowError::validation(format!(
                    "safe access '?.{}' requires a nullable parent value",
                    path_segment.property_name
                )));
            }

            reference_type.value_type = remove_null_from_type(&reference_type.value_type, context.schemas)?;
        } else if is_nullable_type(&reference_type.value_type, context.schemas)? {
            return Err(WorkflowError::validation(format!(
                "property '{}' must be accessed with '?.' because the parent is nullable",
                path_segment.property_name
            )));
        }

        reference_type.value_type = property_type(&reference_type.value_type, &path_segment.property_name, context.schemas)?;
    }

    Ok(reference_type)
}

fn analyze_reference_dependencies(reference: &ReferenceExpression) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();

    if let ReferenceRoot::Agent(agent_name) = &reference.root {
        dependencies.insert(agent_name.clone());
    }

    dependencies
}

fn lookup_field_type(fields: &[TypeField], field_name: &str, scope_name: &str) -> Result<TypeExpression, WorkflowError> {
    fields
        .iter()
        .find(|field| field.name == field_name)
        .map(|field| field.value_type.clone())
        .ok_or_else(|| WorkflowError::validation(format!("unknown {scope_name} reference '{field_name}'")))
}

#[cfg(test)]
mod tests {
    use super::compile_workflow;
    use crate::parser::parse_workflow;
    use std::path::PathBuf;

    fn compile(source: &str) -> Result<crate::compiler::CompiledWorkflow, crate::error::WorkflowError> {
        let workflow = parse_workflow(source)?;
        compile_workflow(workflow, PathBuf::from("."))
    }

    #[test]
    fn rejects_secret_usage_in_prompt_interpolation() {
        let error = compile(
            r#"
            secrets {
                api_key: string
            }

            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent greeting {
                model: openai("gpt-4.1-mini")
                prompt: "Hello {{ secrets.api_key }}"
                output: string
            }

            output {
                greeting: agent.greeting
            }
            "#,
        )
        .expect_err("workflow should be rejected");

        assert!(
            error.to_string().contains("secrets cannot be referenced in prompt"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_missing_safe_access_on_nullable_values() {
        let error = compile(
            r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            agent summary {
                model: openai("gpt-4.1-mini")
                prompt: "Write a summary"
                output: {
                    nested: {
                        value: string
                    } | null
                }
            }

            output {
                value: agent.summary.nested.value
            }
            "#,
        )
        .expect_err("workflow should be rejected");

        assert!(error.to_string().contains("must be accessed with '?.'"));
    }

    #[test]
    fn compiles_all_v2_example_workflows() {
        let example_directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workflows/v2");
        let example_files = [
            "agent_for_loop.ai",
            "context_compaction.ai",
            "inference.ai",
            "minimum.ai",
            "multiline_string.ai",
            "multiple_providers.ai",
            "parallel_agents.ai",
            "schema.ai",
            "schema_types.ai",
            "secrets.ai",
            "string_interpolation.ai",
            "structured_output.ai",
            "template_function.ai",
            "tools.ai",
        ];

        for example_file in example_files {
            let example_path = example_directory.join(example_file);
            let source = std::fs::read_to_string(&example_path).expect("example workflow should be readable");
            let workflow = parse_workflow(&source).unwrap_or_else(|error| panic!("example '{example_file}' should parse: {error}"));

            compile_workflow(
                workflow,
                example_path.parent().expect("example path should have a parent").to_path_buf(),
            )
            .unwrap_or_else(|error| panic!("example '{example_file}' should compile: {error}"));
        }
    }

    #[test]
    fn rejects_template_binding_mismatches() {
        let base_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workflows/v2");
        let workflow = parse_workflow(
            r#"
            provider openai {
                driver: "openai"
                models: ["gpt-4.1-mini"]
            }

            input {
                study_name: string
            }

            agent brief {
                model: openai("gpt-4.1-mini")
                prompt: template("prompts/research_brief.md", {
                    study_name: input.study_name
                })
                output: string
            }

            output {
                brief: agent.brief
            }
            "#,
        )
        .expect("workflow should parse");
        let error = compile_workflow(workflow, base_path).expect_err("workflow should be rejected");

        assert!(error.to_string().contains("bindings must exactly match placeholders"));
    }
}
